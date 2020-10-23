#!/usr/bin/env bash

set -ex

KNATIVE_SERVING_VERSION=${KNATIVE_SERVING_VERSION:-0.17.2}
KNATIVE_EVENTING_VERSION=${KNATIVE_EVENTING_VERSION:-0.17.5}
KOURIER_VERSION=${KOURIER_VERSION:-0.17.0}
EVENTING_KAFKA_VERSION=${EVENTING_KAFKA_VERSION:-nightly}
KAFKA_NS=${KAFKA_NS:-kafka}
DROGUE_NS=${DROGUE_NS:-drogue-iot}

DEPLOY_DIR="$(dirname "${BASH_SOURCE[0]}")/../deploy/02-deploy"

# Knative Serving
kubectl apply -f https://github.com/knative/serving/releases/download/v$KNATIVE_SERVING_VERSION/serving-crds.yaml
kubectl apply -f https://github.com/knative/serving/releases/download/v$KNATIVE_SERVING_VERSION/serving-core.yaml
kubectl wait deployment --all --timeout=-1s --for=condition=Available -n knative-serving

# Kourier ingress for Knative Serving
kubectl apply -f https://github.com/knative/net-kourier/releases/download/v$KOURIER_VERSION/kourier.yaml
kubectl wait deployment --all --timeout=-1s --for=condition=Available -n kourier-system
# deployment for net-kourier gets deployed to namespace knative-serving
kubectl wait deployment --all --timeout=-1s --for=condition=Available -n knative-serving

INGRESS_HOST=$(kubectl -n kourier-system get service kourier -o jsonpath='{.status.loadBalancer.ingress[0].ip}')
while [ -z $INGRESS_HOST ]; do
  sleep 5
  echo "Run 'minikube tunnel' in another shell"
  INGRESS_HOST=$(kubectl -n kourier-system get service kourier -o jsonpath='{.status.loadBalancer.ingress[0].ip}')
done

echo "The INGRESS_HOST is $INGRESS_HOST"
kubectl patch configmap/config-network \
  --namespace knative-serving \
  --type merge \
  --patch '{"data":{"ingress.class":"kourier.ingress.networking.knative.dev"}}'

KNATIVE_DOMAIN=$INGRESS_HOST.nip.io
echo "The KNATIVE_DOMAIN $KNATIVE_DOMAIN"
kubectl patch configmap -n knative-serving config-domain -p "{\"data\": {\"$KNATIVE_DOMAIN\": \"\"}}"

# Knative Eventing
kubectl apply -f https://github.com/knative/eventing/releases/download/v$KNATIVE_EVENTING_VERSION/eventing-crds.yaml
kubectl apply -f https://github.com/knative/eventing/releases/download/v$KNATIVE_EVENTING_VERSION/eventing-core.yaml
kubectl wait deployment --all --timeout=-1s --for=condition=Available -n knative-eventing

# Strimzi
if ! kubectl get ns $KAFKA_NS >/dev/null 2>&1; then kubectl create ns $KAFKA_NS; fi
if ! kubectl -n $KAFKA_NS get deploy/strimzi-cluster-operator >/dev/null 2>&1; then
  kubectl apply -f "https://strimzi.io/install/latest?namespace=$KAFKA_NS" -n $KAFKA_NS
  # the rest is required to watch all namespaces
  kubectl -n $KAFKA_NS set env deploy/strimzi-cluster-operator STRIMZI_NAMESPACE=\*
  if ! kubectl get clusterrolebinding strimzi-cluster-operator-namespaced >/dev/null 2>&1; then
    kubectl create clusterrolebinding strimzi-cluster-operator-namespaced \
      --clusterrole=strimzi-cluster-operator-namespaced \
      --serviceaccount $KAFKA_NS:strimzi-cluster-operator
    kubectl create clusterrolebinding strimzi-cluster-operator-entity-operator-delegation \
      --clusterrole=strimzi-entity-operator \
      --serviceaccount $KAFKA_NS:strimzi-cluster-operator
    kubectl create clusterrolebinding strimzi-cluster-operator-topic-operator-delegation \
      --clusterrole=strimzi-topic-operator \
      --serviceaccount $KAFKA_NS:strimzi-cluster-operator
  fi
fi
kubectl wait deployment --all --timeout=-1s --for=condition=Available -n $KAFKA_NS

# Knative Kafka resources
EVENTING_KAFKA_SOURCE_URL="https://github.com/knative/eventing-contrib/releases/download/v${EVENTING_KAFKA_VERSION}/kafka-source.yaml"
EVENTING_KAFKA_CHANNEL_URL="https://github.com/knative/eventing-contrib/releases/download/v${EVENTING_KAFKA_VERSION}/kafka-channel.yaml"
if [[ ${EVENTING_KAFKA_VERSION} == "nightly" ]]; then
  EVENTING_KAFKA_SOURCE_URL="https://knative-nightly.storage.googleapis.com/eventing-kafka/latest/source.yaml"
  EVENTING_KAFKA_CHANNEL_URL="https://knative-nightly.storage.googleapis.com/eventing-kafka/latest/channel-consolidated.yaml"
fi
curl -L ${EVENTING_KAFKA_SOURCE_URL} \
  | sed 's/namespace: .*/namespace: knative-eventing/' \
  | kubectl apply -f - -n knative-eventing
curl -L ${EVENTING_KAFKA_CHANNEL_URL} \
    | sed 's/REPLACE_WITH_CLUSTER_URL/kafka-eventing-kafka-bootstrap.knative-eventing:9092/' \
    | kubectl apply -f -
kubectl wait deployment --all --timeout=-1s --for=condition=Available -n knative-eventing

# Create workspace for endpoints
if ! kubectl get ns $DROGUE_NS >/dev/null 2>&1; then
  kubectl create namespace $DROGUE_NS
  kubectl label namespace $DROGUE_NS bindings.knative.dev/include=true
fi

# Create configuration
kubectl -n $DROGUE_NS apply -f $DEPLOY_DIR/00-config

# Create kafka cluster
kubectl apply -f $DEPLOY_DIR/01-kafka/010-Kafka.yaml
kubectl patch kafka -n knative-eventing kafka-eventing -p '[{"op": "remove", "path": "/spec/kafka/listeners/external"}]' --type json
kubectl wait kafka --all --for=condition=Ready --timeout=-1s -n knative-eventing

# Create InfluxDB
kubectl -n $DROGUE_NS apply -f $DEPLOY_DIR/02-influxdb

# Create Grafana dashboard
for f in $(ls $DEPLOY_DIR/03-dashboard); do
  if [[ ! $f =~ 'Route' ]]; then
    kubectl -n $DROGUE_NS apply -f $DEPLOY_DIR/03-dashboard/$f
  fi
done
kubectl patch svc -n $DROGUE_NS grafana -p "{\"spec\": {\"type\": \"NodePort\"}}"

# Create needed knative resources
kubectl -n $DROGUE_NS apply -f $DEPLOY_DIR/04-knative

# Create the http endpoint
kubectl -n $DROGUE_NS apply -f $DEPLOY_DIR/05-endpoints/http

# Create the mqtt endpoint
for f in $(ls $DEPLOY_DIR/05-endpoints/mqtt); do
  if [[ ! $f =~ 'Route' ]]; then
    kubectl -n $DROGUE_NS apply -f $DEPLOY_DIR/05-endpoints/mqtt/$f
  fi
done
kubectl -n $DROGUE_NS patch svc mqtt-endpoint -p "{\"spec\": {\"type\": \"NodePort\"}}"

# Provide a TLS certificate for the MQTT endpoint
if ! kubectl -n $DROGUE_NS get secret mqtt-endpoint-tls >/dev/null 2>&1; then
  openssl req -x509 -nodes -days 365 -newkey rsa:2048 -keyout tls_tmp.key -out tls.crt -subj "/CN=foo.bar.com" -addext $(minikube service -n $DROGUE_NS --url mqtt-endpoint | awk -F[/:] '{print "subjectAltName=DNS:" $4 ".nip.io"}')
  openssl rsa -in tls_tmp.key -out tls.key
  kubectl -n $DROGUE_NS create secret tls mqtt-endpoint-tls --key tls.key --cert tls.crt
fi

# Deploy the console
kubectl -n $DROGUE_NS apply -f $DEPLOY_DIR/06-console
kubectl -n $DROGUE_NS patch svc console-backend -p "{\"spec\": {\"type\": \"NodePort\"}}"
kubectl -n $DROGUE_NS patch svc console-frontend -p "{\"spec\": {\"type\": \"NodePort\"}}"
kubectl -n $DROGUE_NS set env deployment/console-frontend "BACKEND_URL=$(minikube service -n $DROGUE_NS --url console-backend)"
kubectl -n $DROGUE_NS set env deployment/console-backend "HTTP_ENDPOINT_URL=$(kubectl get ksvc -n $DROGUE_NS http-endpoint -o jsonpath='{.status.url}')"
kubectl -n $DROGUE_NS set env deployment/console-backend "MQTT_ENDPOINT_HOST=$(minikube service -n $DROGUE_NS --url mqtt-endpoint | awk -F[/:] '{print $4 ".nip.io"}')"
kubectl -n $DROGUE_NS set env deployment/console-backend "MQTT_ENDPOINT_PORT=$(minikube service -n $DROGUE_NS --url mqtt-endpoint | awk -F[/:] '{print $5}')"

kubectl wait ksvc --all --timeout=-1s --for=condition=Ready -n $DROGUE_NS
#kubectl -n $DROGUE_NS set env deploy/mqtt-endpoint DISABLE_TLS=true
kubectl wait deployment --all --timeout=-1s --for=condition=Available -n $DROGUE_NS

# Dump out the dashboard URL and sample commands for http and mqtt
set +x
echo ""
echo "Console:"
echo "  $(minikube service -n $DROGUE_NS --url console-backend)"
echo ""
echo "Login to Grafana:"
echo "  url:      $(minikube service -n $DROGUE_NS --url grafana)"
echo "  username: admin"
echo "  password: admin123456"
echo "Search for the 'Knative test' dashboard"
echo ""
echo "At a shell prompt, try these commands:"
echo "  http POST $(kubectl get ksvc -n $DROGUE_NS http-endpoint -o jsonpath='{.status.url}')/publish/foo temp:=44"
minikube service -n $DROGUE_NS --url mqtt-endpoint | awk -F[/:] '{print "  mqtt pub -v -h " $4 ".nip.io -p " $5 " -s --cafile tls.crt -t temp -m '\''{\"temp\":42}'\'' -V 3"}'
