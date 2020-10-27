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

INGRESS_HOST=$(kubectl -n kourier-system get service kourier -o jsonpath='{.status.loadBalancer.ingress[0].hostname}')
while [ -z $INGRESS_HOST ]; do
  sleep 5
  echo "Waiting for Kourier ingress to get ready!"
  INGRESS_HOST=$(kubectl -n kourier-system get service kourier -o jsonpath='{.status.loadBalancer.ingress[0].hostname}')
done

echo "The KNATIVE_DOMAIN is $INGRESS_HOST"
kubectl patch configmap/config-network \
  --namespace knative-serving \
  --type merge \
  --patch '{"data":{"ingress.class":"kourier.ingress.networking.knative.dev"}}'

# Use magic DNS
kubectl apply --filename https://github.com/knative/serving/releases/download/v$KNATIVE_SERVING_VERSION/serving-default-domain.yaml

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

# Create kafka cluster
kubectl apply -f $DEPLOY_DIR/01-kafka/010-Kafka.yaml
kubectl patch kafka -n knative-eventing kafka-eventing -p '[{"op": "remove", "path": "/spec/kafka/listeners/external"}]' --type json
kubectl wait kafka --all --for=condition=Ready --timeout=-1s -n knative-eventing

# Create InfluxDB
kubectl -n $DROGUE_NS apply -f $DEPLOY_DIR/02-influxdb

# Create Grafana dashboard
kubectl -n $DROGUE_NS apply -f $DEPLOY_DIR/03-dashboard

# Create needed knative resources
kubectl -n $DROGUE_NS apply -f $DEPLOY_DIR/04-knative

# Create the http endpoint
kubectl -n $DROGUE_NS apply -f $DEPLOY_DIR/05-endpoints/http

# Create the MQTT endpoint
kubectl -n $DROGUE_NS apply -f $DEPLOY_DIR/05-endpoints/mqtt

# Provide a TLS certificate for the MQTT endpoint
if ! kubectl -n $DROGUE_NS get secret mqtt-endpoint-tls >/dev/null 2>&1; then
  openssl req -x509 -nodes -days 365 -newkey rsa:2048 -keyout tls_tmp.key -out tls.crt -subj "/CN=foo.bar.com" -addext "subjectAltName = DNS:$(kubectl get route -n drogue-iot mqtt-endpoint -o jsonpath='{.status.ingress[0].host}')"
  openssl rsa -in tls_tmp.key -out tls.key
  kubectl -n $DROGUE_NS create secret tls mqtt-endpoint-tls --key tls.key --cert tls.crt
fi

# Create the Console endpoints
kubectl -n $DROGUE_NS apply -f $DEPLOY_DIR/06-console
kubectl -n $DROGUE_NS apply -f $DEPLOY_DIR/06-console/ocp

kubectl -n $DROGUE_NS set env deployment/console-frontend "BACKEND_URL=https://$(oc get route console-backend -o 'jsonpath={ .spec.host }')" "CLUSTER_DOMAIN-"

kubectl wait ksvc --all --timeout=-1s --for=condition=Ready -n $DROGUE_NS
kubectl wait deployment --all --timeout=-1s --for=condition=Available -n $DROGUE_NS


# Dump out the dashboard URL and sample commands for http and mqtt
set +x
echo ""
echo "Console:"
echo "  $(kubectl -n $DROGUE_NS get routes console -o jsonpath={.spec.host})"
echo ""
echo "Login to Grafana:"
echo "  url:      $(kubectl -n $DROGUE_NS get routes grafana -o jsonpath={.spec.host})"
echo "  username: admin"
echo "  password: admin123456"
echo "Search for the 'Knative test' dashboard"
echo ""
echo "At a shell prompt, try these commands:"
echo "  http POST $(kubectl get ksvc -n $DROGUE_NS http-endpoint -o jsonpath='{.status.url}')/publish/foo temp:=44"
echo "  mqtt pub -v -h $(kubectl get route -n drogue-iot mqtt-endpoint -o jsonpath='{.status.ingress[0].host}') -p 443 -s --cafile tls.crt -t temp -m '{\"temp\":42}' -V 3"
