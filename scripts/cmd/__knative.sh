#!/usr/bin/env bash

set -e

: "${KNATIVE_SERVING_VERSION:=0.24.1}"
: "${KNATIVE_EVENTING_VERSION:=0.24.3}"
: "${KOURIER_VERSION:=0.24.0}"
: "${EVENTING_KAFKA_VERSION:=0.24.5}"
: "${EVENTING_KAFKA_BROKER_VERSION:=0.24.1}"

# Knative Serving

progress -n "  🏗 Deploying Knative serving ... "
kubectl apply -f https://github.com/knative/serving/releases/download/v$KNATIVE_SERVING_VERSION/serving-crds.yaml
kubectl apply -f https://github.com/knative/serving/releases/download/v$KNATIVE_SERVING_VERSION/serving-core.yaml
progress "done!"

progress -n "  ⏳ Waiting for Knative serving to become ready ... "
kubectl wait deployment --all --timeout=-1s --for=condition=Available -n knative-serving
progress "done!"

# Kourier ingress for Knative Serving
case $CLUSTER in
kubernetes)
    curl -s -L https://github.com/knative/net-kourier/releases/download/v$KOURIER_VERSION/kourier.yaml | sed -e 's/LoadBalancer/NodePort/g' | kubectl apply -f -
    INGRESS_COMMAND="kubectl get nodes -o jsonpath='{.items[0].status.addresses[?(@.type == \"ExternalIP\")].address}'"
    ;;
kind)
    curl -s -L https://github.com/knative/net-kourier/releases/download/v$KOURIER_VERSION/kourier.yaml | sed -e 's/LoadBalancer/NodePort/g' | kubectl apply -f -
    INGRESS_COMMAND="kubectl get node kind-control-plane -o jsonpath='{.status.addresses[?(@.type == \"InternalIP\")].address}' | awk '// {print \$1}'"
    ;;
minikube)
    kubectl apply -f https://github.com/knative/net-kourier/releases/download/v$KOURIER_VERSION/kourier.yaml
    INGRESS_COMMAND="kubectl -n kourier-system get service kourier -o jsonpath='{.status.loadBalancer.ingress[0].ip}'"
    ;;
*)
    kubectl apply -f https://github.com/knative/net-kourier/releases/download/v$KOURIER_VERSION/kourier.yaml
    INGRESS_COMMAND="kubectl -n kourier-system get service kourier -o jsonpath='{.status.loadBalancer.ingress[0].hostname}'"
    ;;
esac

if [[ "$MINIMIZE" == true ]]; then
    kubectl -n knative-serving set resources deployment activator autoscaler controller webhook --requests=cpu=0
fi

# Wait for deployment to finish
progress -n "  ⏳ Waiting for Knative serving and Kourier to become ready ... "
kubectl wait deployment --all --timeout=-1s --for=condition=Available -n kourier-system
# deployment for net-kourier gets deployed to namespace knative-serving
kubectl wait deployment --all --timeout=-1s --for=condition=Available -n knative-serving
progress "done!"

# shellcheck disable=SC2086
INGRESS_HOST=$(eval $INGRESS_COMMAND)
while [ -z "$INGRESS_HOST" ]; do
    sleep 5

    case $CLUSTER in
    minikube)
        progress "  🔌 Waiting for Kourier ingress to get ready! If you're running minikube, run 'minikube tunnel' in another shell!"
        ;;
    *)
        progress "  🔌 Waiting for Kourier ingress to get ready!"
        ;;
    esac

    # shellcheck disable=SC2086
    INGRESS_HOST=$(eval $INGRESS_COMMAND)
done

echo "The INGRESS_HOST is $INGRESS_HOST"
kubectl patch configmap/config-network \
    --namespace knative-serving \
    --type merge \
    --patch '{"data":{"ingress.class":"kourier.ingress.networking.knative.dev"}}'

case $CLUSTER in
kubernetes)
    KNATIVE_DOMAIN=$INGRESS_HOST.nip.io
    echo "The KNATIVE_DOMAIN $KNATIVE_DOMAIN"
    kubectl patch configmap -n knative-serving config-domain -p "{\"data\": {\"$KNATIVE_DOMAIN\": \"\"}}"
    ;;
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
    ;;
esac

# Knative Eventing

progress -n "  🏗 Deploying Knative eventing ... "

kubectl apply -f https://github.com/knative/eventing/releases/download/v$KNATIVE_EVENTING_VERSION/eventing-crds.yaml
kubectl apply -f https://github.com/knative/eventing/releases/download/v$KNATIVE_EVENTING_VERSION/eventing-core.yaml
kubectl -n knative-eventing set env deployment/eventing-webhook SINK_BINDING_SELECTION_MODE=inclusion

# Knative Kafka Sink and Source
kubectl apply --filename https://github.com/knative-sandbox/eventing-kafka/releases/download/v${EVENTING_KAFKA_VERSION}/source.yaml
kubectl apply --filename https://github.com/knative-sandbox/eventing-kafka-broker/releases/download/v${EVENTING_KAFKA_BROKER_VERSION}/eventing-kafka-controller.yaml
kubectl apply --filename https://github.com/knative-sandbox/eventing-kafka-broker/releases/download/v${EVENTING_KAFKA_BROKER_VERSION}/eventing-kafka-sink.yaml

progress "done!"

# Wait for eventing deployments
progress -n "  ⏳ Waiting for Knative eventing to become ready ... "
kubectl wait deployment --all --timeout=-1s --for=condition=Available -n knative-eventing
progress "done!"
