#!/usr/bin/env bash

CERT_ALTNAMES=""

case $CLUSTER in
kubernetes)
    MQTT_ENDPOINT_HOST=mqtt-endpoint.$(kubectl get service -n "$DROGUE_NS" mqtt-endpoint -o 'jsonpath={ .status.loadBalancer.ingress[0].ip }').nip.io
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
    die "Unknown Kubernetes platform: $CLUSTER ... unable to extract endpoints"
    ;;
esac

HTTP_ENDPOINT_URL="https://${HTTP_ENDPOINT_HOST}:${HTTP_ENDPOINT_PORT}"
CONSOLE_URL=$(ingress_url_wait "console")
SSO_URL="$(ingress_url_wait "keycloak")"
API_URL="$(ingress_url_wait "api")"

DASHBOARD_URL=$(ingress_url_wait "grafana")

if [[ -z "$SILENT" ]]; then

    echo
    bold "========================================================"
    bold "  Services"
    bold "========================================================"
    echo
    echo "Console:          $CONSOLE_URL"
    echo "SSO:              $SSO_URL"
    echo "API:              $API_URL"
    echo
    echo "HTTP Endpoint:    $HTTP_ENDPOINT_URL"
    echo "MQTT Endpoint:    $MQTT_ENDPOINT_HOST:$MQTT_ENDPOINT_PORT"
    echo
    echo "MQTT Integration: $MQTT_INTEGRATION_HOST:$MQTT_INTEGRATION_PORT"
    echo
    bold "========================================================"
    bold "  Examples"
    bold "========================================================"
    echo
    echo "Grafana:          $DASHBOARD_URL"
    echo

fi
