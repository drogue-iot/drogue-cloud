#!/usr/bin/env bash

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
source "$SCRIPTDIR/common.sh"

CERT_ALTNAMES=""

case $CLUSTER in
   kubernetes)
       MQTT_ENDPOINT_HOST=mqtt-endpoint.$(kubectl get service -n "$DROGUE_NS" mqtt-endpoint  -o 'jsonpath={ .status.loadBalancer.ingress[0].ip }').nip.io
       MQTT_ENDPOINT_PORT=$(kubectl get service -n "$DROGUE_NS" mqtt-endpoint -o jsonpath='{.spec.ports[0].port}')

       MQTT_INTEGRATION_HOST=mqtt-integration.$(kubectl get service -n "$DROGUE_NS" mqtt-integration -o 'jsonpath={ .status.loadBalancer.ingress[0].ip }').nip.io
       MQTT_INTEGRATION_PORT=$(kubectl get service -n "$DROGUE_NS" mqtt-integration -o jsonpath='{.spec.ports[0].port}')

       HTTP_ENDPOINT_HOST=http-endpoint.$(kubectl get service -n "$DROGUE_NS" http-endpoint -o 'jsonpath={ .status.loadBalancer.ingress[0].ip }').nip.io
       HTTP_ENDPOINT_PORT=$(kubectl get service -n "$DROGUE_NS" http-endpoint -o jsonpath='{.spec.ports[0].port}')
       ;;
   kind)
       DOMAIN=$(kubectl get node kind-control-plane -o jsonpath='{.status.addresses[?(@.type == "InternalIP")].address}').nip.io
       MQTT_ENDPOINT_HOST=mqtt-endpoint.$DOMAIN
       MQTT_ENDPOINT_PORT=$(kubectl get service -n "$DROGUE_NS" mqtt-endpoint -o jsonpath='{.spec.ports[0].nodePort}')
       MQTT_INTEGRATION_HOST=mqtt-integration.$DOMAIN
       MQTT_INTEGRATION_PORT=$(kubectl get service -n "$DROGUE_NS" mqtt-integration -o jsonpath='{.spec.ports[0].nodePort}')
       HTTP_ENDPOINT_HOST=http-endpoint.$DOMAIN
       HTTP_ENDPOINT_PORT=$(kubectl get service -n "$DROGUE_NS" http-endpoint -o jsonpath='{.spec.ports[0].nodePort}')
       ;;
   minikube)
        MQTT_ENDPOINT_HOST=$(minikube service -n "$DROGUE_NS" --url mqtt-endpoint | awk -F[/:] '{print $4 ".nip.io"}')
        MQTT_ENDPOINT_PORT=$(minikube service -n "$DROGUE_NS" --url mqtt-endpoint | awk -F[/:] '{print $5}')
        MQTT_INTEGRATION_HOST=$(minikube service -n "$DROGUE_NS" --url mqtt-integration | awk -F[/:] '{print $4 ".nip.io"}')
        MQTT_INTEGRATION_PORT=$(minikube service -n "$DROGUE_NS" --url mqtt-integration | awk -F[/:] '{print $5}')
        HTTP_ENDPOINT_IP=$(minikube service -n "$DROGUE_NS" --url http-endpoint | awk -F[/:] '{print $4}')
        CERT_ALTNAMES="$CERT_ALTNAMES IP:$HTTP_ENDPOINT_IP, "
        HTTP_ENDPOINT_HOST=$(minikube service -n "$DROGUE_NS" --url http-endpoint | awk -F[/:] '{print $4 ".nip.io"}')
        HTTP_ENDPOINT_PORT=$(minikube service -n "$DROGUE_NS" --url http-endpoint | awk -F[/:] '{print $5}')
        ;;
   openshift)
        MQTT_ENDPOINT_HOST=$(kubectl get route -n "$DROGUE_NS" mqtt-endpoint -o jsonpath='{.status.ingress[0].host}')
        MQTT_ENDPOINT_PORT=443
        MQTT_INTEGRATION_HOST=$(kubectl get route -n "$DROGUE_NS" mqtt-integration -o jsonpath='{.status.ingress[0].host}')
        MQTT_INTEGRATION_PORT=443
        HTTP_ENDPOINT_HOST=$(kubectl get route -n "$DROGUE_NS" http-endpoint -o jsonpath='{.status.ingress[0].host}')
        HTTP_ENDPOINT_PORT=443
        ;;
   *)
        echo "Unknown Kubernetes platform: $CLUSTER ... unable to extract endpoints"
        exit 1
        ;;
esac;


#
# Wait for SSO
#
SSO_URL="$(route_url "keycloak")"
while [ -z "$SSO_URL" ]; do
    sleep 5
    echo "Waiting for Keycloak ingress to get ready! If you're running minikube, run 'minikube tunnel' in another shell and ensure that you have the ingress addon enabled."
    SSO_URL="$(route_url "keycloak")"
done
SSO_HOST="${SSO_URL/#http:\/\//}"
SSO_HOST="${SSO_HOST/#https:\/\//}"

#
# Wait for API
#
API_URL="$(ingress_url "api")"
while [ -z "$API_URL" ]; do
    sleep 5
    echo "Waiting for API ingress to get ready! If you're running minikube, run 'minikube tunnel' in another shell and ensure that you have the ingress addon enabled."
    API_URL="$(ingress_url "api")"
done
API_HOST="${API_URL/#http:\/\//}"
API_HOST="${API_HOST/#https:\/\//}"

HTTP_ENDPOINT_URL="https://${HTTP_ENDPOINT_HOST}:${HTTP_ENDPOINT_PORT}"
COMMAND_ENDPOINT_URL=$(service_url "command-endpoint")
BACKEND_URL=$(service_url "console-backend")
CONSOLE_URL=$(service_url "console")
DASHBOARD_URL=$(service_url "grafana")

if [ "$CLUSTER" == "kubernetes" ]; then
    CONSOLE_URL=${API_URL}
    CONSOLE_HOST=${API_HOST}
    COMMAND_ENDPOINT_URL=${API_URL}
    BACKEND_URL=${API_URL}
fi

if [[ -z "$SILENT" ]]; then

  echo
  bold "========================================================"
  bold "  Services"
  bold "========================================================"
  echo
  echo "Console:          $CONSOLE_URL"
  echo "SSO:              $SSO_URL ($SSO_HOST)"
  echo
  echo "API:              $API_URL ($API_HOST)"
  echo "Backend:          $BACKEND_URL"
  echo
  echo "Command Endpoint: $COMMAND_ENDPOINT_URL"
  echo
  echo "HTTP Endpoint:    $HTTP_ENDPOINT_URL"
  echo "MQTT Endpoint:    $MQTT_ENDPOINT_HOST:$MQTT_ENDPOINT_PORT"
  echo
  echo "MQTT Integration: $MQTT_INTEGRATION_HOST:$MQTT_INTEGRATION_PORT"
  echo

fi
