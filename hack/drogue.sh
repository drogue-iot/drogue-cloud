#!/usr/bin/env bash

set -ex

: "${CLUSTER:=minikube}"
: "${MQTT:=true}"
: "${INSTALL_STRIMZI:=true}"
: "${INSTALL_KNATIVE:=true}"
: "${INSTALL_KEYCLOAK_OPERATOR:=true}"

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
DEPLOYDIR="$SCRIPTDIR/.."

source "$SCRIPTDIR/common.sh"

# Create the namespace first
if ! kubectl get ns "$DROGUE_NS" >/dev/null 2>&1; then
  kubectl create namespace "$DROGUE_NS"
  kubectl label namespace "$DROGUE_NS" bindings.knative.dev/include=true
fi

# install additional components

[[ "$INSTALL_STRIMZI" == true ]] && source "$SCRIPTDIR/strimzi.sh"
[[ "$INSTALL_KNATIVE" == true ]] && source "$SCRIPTDIR/knative.sh"
[[ "$INSTALL_KEYCLOAK_OPERATOR" == true ]] && source "$SCRIPTDIR/sso.sh"
source "$SCRIPTDIR/registry.sh"

# Install Drogue components (sources and services)

kubectl -n "$DROGUE_NS" apply -k "$SCRIPTDIR/../deploy/$CLUSTER/"

# Wait for the HTTP endpoint to become ready

kubectl -n "$DROGUE_NS" wait --timeout=-1s --for=condition=Ready ksvc/http-endpoint
HTTP_ENDPOINT_URL=$(eval "kubectl get ksvc -n $DROGUE_NS http-endpoint -o jsonpath='{.status.url}'")

case $CLUSTER in
   kind)
       DOMAIN=$(kubectl get node kind-control-plane -o jsonpath='{.status.addresses[?(@.type == "InternalIP")].address}').nip.io
       MQTT_ENDPOINT_HOST=mqtt-endpoint.$DOMAIN
       MQTT_ENDPOINT_PORT=$(kubectl get service -n "$DROGUE_NS" mqtt-endpoint -o jsonpath='{.spec.ports[0].nodePort}')
       HTTP_ENDPOINT_PORT=$(kubectl get service -n kourier-system kourier -o jsonpath='{.spec.ports[?(@.name == "http2")].nodePort}')
       HTTP_ENDPOINT_URL=${HTTP_ENDPOINT_URL}:${HTTP_ENDPOINT_PORT}

       BACKEND_PORT=$(kubectl get service -n "$DROGUE_NS" console-backend -o jsonpath='{.spec.ports[0].nodePort}')
       BACKEND_URL=http://console-backend.$DOMAIN:$BACKEND_PORT
       ;;
   minikube)
        MQTT_ENDPOINT_HOST=$(eval minikube service -n "$DROGUE_NS" --url mqtt-endpoint | awk -F[/:] '{print $4 ".nip.io"}')
        MQTT_ENDPOINT_PORT=$(eval minikube service -n "$DROGUE_NS" --url mqtt-endpoint | awk -F[/:] '{print $5}')
        BACKEND_URL=$(eval minikube service -n "$DROGUE_NS" --url console-backend)
        ;;
   openshift)
        MQTT_ENDPOINT_HOST=$(eval kubectl get route -n drogue-iot mqtt-endpoint -o jsonpath='{.status.ingress[0].host}')
        MQTT_ENDPOINT_PORT=443
        HTTP_ENDPOINT_URL=$(kubectl get ksvc -n $DROGUE_NS http-endpoint -o jsonpath='{.status.url}' | sed 's/http:/https:/')
        BACKEND_URL="https://$(kubectl get route -n "$DROGUE_NS" console-backend -o 'jsonpath={ .spec.host }')"
        ;;
   *)
        echo "Unknown Kubernetes platform: $CLUSTER ... unable to extract endpoints"
        exit 1
        ;;
esac;

# Provide a TLS certificate for the MQTT endpoint

if  [ "$MQTT" = true ] && [ "$(kubectl -n $DROGUE_NS get secret mqtt-endpoint-tls --ignore-not-found)" == "" ] ; then
  openssl req -x509 -nodes -days 365 -newkey rsa:2048 -keyout tls_tmp.key -out tls.crt -subj "/CN=foo.bar.com" -addext "subjectAltName = DNS:$MQTT_ENDPOINT_HOST"
  openssl rsa -in tls_tmp.key -out tls.key
  kubectl -n "$DROGUE_NS" create secret tls mqtt-endpoint-tls --key tls.key --cert tls.crt
fi

# Update the console endpoints

kubectl -n "$DROGUE_NS" set env deployment/console-backend "HTTP_ENDPOINT_URL=$HTTP_ENDPOINT_URL"
kubectl -n "$DROGUE_NS" set env deployment/console-backend "MQTT_ENDPOINT_HOST=$MQTT_ENDPOINT_HOST"
kubectl -n "$DROGUE_NS" set env deployment/console-backend "MQTT_ENDPOINT_PORT=$MQTT_ENDPOINT_PORT"
kubectl -n "$DROGUE_NS" set env deployment/console-frontend "BACKEND_URL=$BACKEND_URL"

# wait for all necessary components

kubectl wait ksvc --all --timeout=-1s --for=condition=Ready -n "$DROGUE_NS"
kubectl wait deployment --all --timeout=-1s --for=condition=Available -n "$DROGUE_NS"

# show status

source "$SCRIPTDIR/status.sh"
