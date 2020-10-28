#!/usr/bin/env bash

set -ex

KNATIVE_SERVING_VERSION=${KNATIVE_SERVING_VERSION:-0.17.2}
KNATIVE_EVENTING_VERSION=${KNATIVE_EVENTING_VERSION:-0.17.5}
KOURIER_VERSION=${KOURIER_VERSION:-0.17.0}
EVENTING_KAFKA_VERSION=${EVENTING_KAFKA_VERSION:-nightly}
KAFKA_NS=${KAFKA_NS:-kafka}

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
: "${CLUSTER:=minikube}"

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

case $CLUSTER in
   minikube)
        INGRESS_COMMAND="kubectl -n kourier-system get service kourier -o jsonpath='{.status.loadBalancer.ingress[0].ip}'"
        ;;
   *)
        INGRESS_COMMAND="kubectl -n kourier-system get service kourier -o jsonpath='{.status.loadBalancer.ingress[0].hostname}'"
        ;;
esac;

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