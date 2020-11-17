#!/usr/bin/env bash

set -ex

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
DEPLOY_DIR="$(dirname "${BASH_SOURCE[0]}")/../deploy/02-deploy"
CLUSTER=${CLUSTER:-"minikube"}
MQTT=true
CONSOLE=true
HELM=false
HELM_ARGS="--values $SCRIPTDIR/../deploy/helm/drogue-iot/profile-openshift.yaml"

source "$SCRIPTDIR/common.sh"
source "$SCRIPTDIR/knative.sh"
source "$SCRIPTDIR/registry.sh"

# Create workspace for endpoints
if ! kubectl get ns $DROGUE_NS >/dev/null 2>&1; then
  kubectl create namespace $DROGUE_NS
  kubectl label namespace $DROGUE_NS bindings.knative.dev/include=true
fi

# Create kafka cluster
kubectl apply -f $DEPLOY_DIR/01-kafka/010-Kafka.yaml
kubectl patch kafka -n knative-eventing kafka-eventing -p '[{"op": "remove", "path": "/spec/kafka/listeners/external"}]' --type json
kubectl wait kafka --all --for=condition=Ready --timeout=-1s -n knative-eventing

# Install Drogue components (sources and services)

if [ "$HELM" = true ] ; then
  if [ "$MQTT" = true ] ; then
    HELM_ARGS+=" --set sources.mqtt.enabled=true"
  fi

  if [ "$CONSOLE" = true ] ; then
    HELM_ARGS+=" --set services.console.enabled=true"
  fi

  helm install --dependency-update -n $DROGUE_NS $HELM_ARGS drogue-iot $SCRIPTDIR/../deploy/helm/drogue-iot/
else
  kubectl -n $DROGUE_NS apply -k $SCRIPTDIR/../deploy/$CLUSTER/
fi

# Wait for the HTTP endpoint to become ready

kubectl -n $DROGUE_NS wait --timeout=-1s --for=condition=Ready ksvc/http-endpoint
HTTP_ENDPOINT_URL=$(eval "kubectl get ksvc -n $DROGUE_NS http-endpoint -o jsonpath='{.status.url}'")

case $CLUSTER in
   kind)
       DOMAIN=$(kubectl get node kind-control-plane -o jsonpath='{.status.addresses[?(@.type == "InternalIP")].address}').nip.io
       MQTT_ENDPOINT_HOST=mqtt-endpoint.$DOMAIN
       MQTT_ENDPOINT_PORT=$(kubectl get service -n $DROGUE_NS mqtt-endpoint -o jsonpath='{.spec.ports[0].nodePort}')
       HTTP_ENDPOINT_PORT=$(kubectl get service -n kourier-system kourier -o jsonpath='{.spec.ports[?(@.name == "http2")].nodePort}')
       HTTP_ENDPOINT_URL=${HTTP_ENDPOINT_URL}:${HTTP_ENDPOINT_PORT}

       BACKEND_PORT=$(kubectl get service -n $DROGUE_NS console-backend -o jsonpath='{.spec.ports[0].nodePort}')
       BACKEND_URL=http://console-backend.$DOMAIN:$BACKEND_PORT
       ;;
   minikube)
        MQTT_ENDPOINT_HOST=$(eval minikube service -n $DROGUE_NS --url mqtt-endpoint | awk -F[/:] '{print $4 ".nip.io"}')
        MQTT_ENDPOINT_PORT=$(eval minikube service -n $DROGUE_NS --url mqtt-endpoint | awk -F[/:] '{print $5}')
        BACKEND_URL=$(eval minikube service -n $DROGUE_NS --url console-backend)
        ;;
   *)
        MQTT_ENDPOINT_HOST=$(eval kubectl get route -n drogue-iot mqtt-endpoint -o jsonpath='{.status.ingress[0].host}')
        MQTT_ENDPOINT_PORT=443
        BACKEND_URL=https://$(eval kubectl get route -n $DROGUE_NS console-backend -o 'jsonpath={ .spec.host }')
        ;;
esac;


# Provide a TLS certificate for the MQTT endpoint

if  [ "$MQTT" = true ] && [ $(kubectl -n $DROGUE_NS get secret mqtt-endpoint-tls --ignore-not-found) != ""] ; then
  openssl req -x509 -nodes -days 365 -newkey rsa:2048 -keyout tls_tmp.key -out tls.crt -subj "/CN=foo.bar.com" -addext "subjectAltName = DNS:$MQTT_ENDPOINT_HOST"
  openssl rsa -in tls_tmp.key -out tls.key
  kubectl -n $DROGUE_NS create secret tls mqtt-endpoint-tls --key tls.key --cert tls.crt
fi

# Create the Console endpoints
if [ $CONSOLE = "true" ] ; then

  # Create the Console endpoints
  kubectl -n $DROGUE_NS set env deployment/console-backend "ENDPOINT_SOURCE-"
  kubectl -n $DROGUE_NS set env deployment/console-backend "HTTP_ENDPOINT_URL=$HTTP_ENDPOINT_URL"

  kubectl -n $DROGUE_NS set env deployment/console-backend "MQTT_ENDPOINT_HOST=$MQTT_ENDPOINT_HOST"
  kubectl -n $DROGUE_NS set env deployment/console-backend "MQTT_ENDPOINT_PORT=$MQTT_ENDPOINT_PORT"

  kubectl -n $DROGUE_NS set env deployment/console-frontend "BACKEND_URL=$BACKEND_URL" "CLUSTER_DOMAIN-"
fi


kubectl wait ksvc --all --timeout=-1s --for=condition=Ready -n $DROGUE_NS
kubectl wait deployment --all --timeout=-1s --for=condition=Available -n $DROGUE_NS

source "$SCRIPTDIR/status.sh"
