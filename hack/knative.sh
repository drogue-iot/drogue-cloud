#!/usr/bin/env bash

set -ex

: "${KNATIVE_SERVING_VERSION:=0.19.0}"
: "${KNATIVE_EVENTING_VERSION:=0.19.1}"
: "${KOURIER_VERSION:=0.19.0}"
: "${EVENTING_KAFKA_VERSION:=0.19.0}"
: "${KAFKA_NS:=kafka}"
: "${CLUSTER:=minikube}"

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"

DEPLOYDIR="$SCRIPTDIR/../deploy"

# Knative Serving
kubectl apply -f https://github.com/knative/serving/releases/download/v$KNATIVE_SERVING_VERSION/serving-crds.yaml
kubectl apply -f https://github.com/knative/serving/releases/download/v$KNATIVE_SERVING_VERSION/serving-core.yaml
kubectl wait deployment --all --timeout=-1s --for=condition=Available -n knative-serving

# Kourier ingress for Knative Serving
case $CLUSTER in
    kind)
        curl -s -L https://github.com/knative/net-kourier/releases/download/v$KOURIER_VERSION/kourier.yaml | sed -e 's/LoadBalancer/NodePort/g' | kubectl apply -f -
        INGRESS_COMMAND="kubectl get node kind-control-plane -o jsonpath='{.status.addresses[?(@.type == \"InternalIP\")].address}'"
        ;;
   minikube)
        kubectl apply -f https://github.com/knative/net-kourier/releases/download/v$KOURIER_VERSION/kourier.yaml
        INGRESS_COMMAND="kubectl -n kourier-system get service kourier -o jsonpath='{.status.loadBalancer.ingress[0].ip}'"
        ;;
   *)
        kubectl apply -f https://github.com/knative/net-kourier/releases/download/v$KOURIER_VERSION/kourier.yaml
        INGRESS_COMMAND="kubectl -n kourier-system get service kourier -o jsonpath='{.status.loadBalancer.ingress[0].hostname}'"
        ;;
esac;

# Wait for deployment to finish
kubectl wait deployment --all --timeout=-1s --for=condition=Available -n kourier-system
# deployment for net-kourier gets deployed to namespace knative-serving
kubectl wait deployment --all --timeout=-1s --for=condition=Available -n knative-serving

INGRESS_HOST=$(eval $INGRESS_COMMAND)
while [ -z $INGRESS_HOST ]; do
  sleep 5
  echo "Waiting for Kourier ingress to get ready! If you're running minikube, run 'minikube tunnel' in another shell"
  INGRESS_HOST=$(eval $INGRESS_COMMAND)
done

echo "The INGRESS_HOST is $INGRESS_HOST"
kubectl patch configmap/config-network \
  --namespace knative-serving \
  --type merge \
  --patch '{"data":{"ingress.class":"kourier.ingress.networking.knative.dev"}}'

case $CLUSTER in
   kind)
        KNATIVE_DOMAIN=$INGRESS_HOST.nip.io
        echo "The KNATIVE_DOMAIN $KNATIVE_DOMAIN"
        kubectl patch configmap -n knative-serving config-domain -p "{\"data\": {\"$KNATIVE_DOMAIN\": \"\"}}"
        ;;
   minikube)
        KNATIVE_DOMAIN=$INGRESS_HOST.nip.io
        echo "The KNATIVE_DOMAIN $KNATIVE_DOMAIN"
        kubectl patch configmap -n knative-serving config-domain -p "{\"data\": {\"$KNATIVE_DOMAIN\": \"\"}}"
        ;;
   *)
        # Use magic DNS
        echo "The KNATIVE_DOMAIN is $INGRESS_HOST"
        kubectl apply --filename https://github.com/knative/serving/releases/download/v$KNATIVE_SERVING_VERSION/serving-default-domain.yaml
esac;

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
curl -L "https://github.com/knative-sandbox/eventing-kafka/releases/download/v${EVENTING_KAFKA_VERSION}/source.yaml" \
  | sed 's/namespace: .*/namespace: knative-eventing/' \
  | kubectl apply -f - -n knative-eventing
kubectl wait deployment --all --timeout=-1s --for=condition=Available -n knative-eventing
curl -L "https://github.com/knative-sandbox/eventing-kafka/releases/download/v${EVENTING_KAFKA_VERSION}/channel-consolidated.yaml" \
    | sed 's/REPLACE_WITH_CLUSTER_URL/kafka-eventing-kafka-bootstrap.knative-eventing:9092/' \
    | kubectl apply -f -
kubectl wait deployment --all --timeout=-1s --for=condition=Available -n knative-eventing

# Create kafka cluster
kubectl -n knative-eventing apply -k "$DEPLOYDIR/knative"
kubectl -n knative-eventing wait kafka --all --for=condition=Ready --timeout=-1s
