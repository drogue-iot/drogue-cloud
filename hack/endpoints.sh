#!/usr/bin/env bash

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
source "$SCRIPTDIR/common.sh"

CERT_ALTNAMES=""

case $CLUSTER in
   kind)
       DOMAIN=$(kubectl get node kind-control-plane -o jsonpath='{.status.addresses[?(@.type == "InternalIP")].address}').nip.io
       MQTT_ENDPOINT_HOST=mqtt-endpoint.$DOMAIN
       MQTT_ENDPOINT_PORT=$(kubectl get service -n "$DROGUE_NS" mqtt-endpoint -o jsonpath='{.spec.ports[0].nodePort}')
       HTTP_ENDPOINT_HOST=http-endpoint.$DOMAIN
       HTTP_ENDPOINT_PORT=$(kubectl get service -n "$DROGUE_NS" http-endpoint -o jsonpath='{.spec.ports[0].nodePort}')
       ;;
   minikube)
        MQTT_ENDPOINT_HOST=$(eval minikube service -n "$DROGUE_NS" --url mqtt-endpoint | awk -F[/:] '{print $4 ".nip.io"}')
        MQTT_ENDPOINT_PORT=$(eval minikube service -n "$DROGUE_NS" --url mqtt-endpoint | awk -F[/:] '{print $5}')
        HTTP_ENDPOINT_IP=$(eval minikube service -n "$DROGUE_NS" --url http-endpoint | awk -F[/:] '{print $4}')
        CERT_ALTNAMES="$CERT_ALTNAMES IP:$HTTP_ENDPOINT_IP, "
        HTTP_ENDPOINT_HOST=$(eval minikube service -n "$DROGUE_NS" --url http-endpoint | awk -F[/:] '{print $4 ".nip.io"}')
        HTTP_ENDPOINT_PORT=$(eval minikube service -n "$DROGUE_NS" --url http-endpoint | awk -F[/:] '{print $5}')
        ;;
   openshift)
        MQTT_ENDPOINT_HOST=$(eval kubectl get route -n "$DROGUE_NS" mqtt-endpoint -o jsonpath='{.status.ingress[0].host}')
        MQTT_ENDPOINT_PORT=443
        HTTP_ENDPOINT_HOST=$(eval kubectl get route -n "$DROGUE_NS" http-endpoint -o jsonpath='{.status.ingress[0].host}')
        HTTP_ENDPOINT_PORT=443
        ;;
   *)
        echo "Unknown Kubernetes platform: $CLUSTER ... unable to extract endpoints"
        exit 1
        ;;
esac;


HTTP_ENDPOINT_URL="https://${HTTP_ENDPOINT_HOST}:${HTTP_ENDPOINT_PORT}"

COMMAND_ENDPOINT_URL=$(service_url "command-endpoint")
BACKEND_URL=$(service_url "console-backend")
CONSOLE_URL=$(service_url "console")
DASHBOARD_URL=$(service_url "grafana")
MGMT_URL=$(service_url "registry")

#
# Wait for SSO
#
SSO_URL="$(ingress_url "keycloak")"
while [ -z "$SSO_URL" ]; do
  sleep 5
  echo "Waiting for Keycloak ingress to get ready! If you're running minikube, run 'minikube tunnel' in another shell and ensure that you have the ingress addon enabled."
  SSO_URL="$(ingress_url "keycloak")"
done
