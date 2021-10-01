#!/usr/bin/env bash

API_URL="$(get_env deploy/console-backend endpoint ENDPOINTS__API_URL)"
CONSOLE_URL="$(get_env deploy/console-backend endpoint ENDPOINTS__CONSOLE_URL)"
 
COAP_ENDPOINT_URL="$(get_env deploy/console-backend endpoint ENDPOINTS__COAP_ENDPOINT_URL)"
COAP_ENDPOINT_HOST="$(echo "$COAP_ENDPOINT_URL" | sed -E -e 's/:[0-9]+$//' -e 's|^coap?://||' )"

MQTT_ENDPOINT_HOST="$(get_env deploy/console-backend endpoint ENDPOINTS__MQTT_ENDPOINT_HOST)"
MQTT_ENDPOINT_PORT="$(get_env deploy/console-backend endpoint ENDPOINTS__MQTT_ENDPOINT_PORT)"
MQTT_INTEGRATION_HOST="$(get_env deploy/console-backend endpoint ENDPOINTS__MQTT_INTEGRATION_HOST)"
MQTT_INTEGRATION_PORT="$(get_env deploy/console-backend endpoint ENDPOINTS__MQTT_INTEGRATION_PORT)"

WEBSOCKET_INTEGRATION_URL="$(get_env deploy/console-backend endpoint ENDPOINTS__WEBSOCKET_INTEGRATION_URL)"
WEBSOCKET_INTEGRATION_HOST="$(echo "$WEBSOCKET_INTEGRATION_URL" | sed -E -e 's/:[0-9]+$//' -e 's|^https?://||' )"

HTTP_ENDPOINT_URL="$(get_env deploy/console-backend endpoint ENDPOINTS__HTTP_ENDPOINT_URL)"
HTTP_ENDPOINT_HOST="$(echo "$HTTP_ENDPOINT_URL" | sed -E -e 's/:[0-9]+$//' -e 's|^https?://||' )"

DASHBOARD_URL="grafana$(detect_domain)"

if [[ -z "$SILENT" ]]; then

    {
        echo
        bold "========================================================"
        bold "  Services"
        bold "========================================================"
        echo
        echo "Console:          $CONSOLE_URL"
        echo "SSO:              $SSO_URL"
        echo "API:              $API_URL"
        echo
        echo "CoAP Endpoint:    $COAP_ENDPOINT_URL ($COAP_ENDPOINT_HOST)"
        echo "HTTP Endpoint:    $HTTP_ENDPOINT_URL ($HTTP_ENDPOINT_HOST)"
        echo "MQTT Endpoint:    $MQTT_ENDPOINT_HOST:$MQTT_ENDPOINT_PORT"
        echo
        echo "MQTT Integration: $MQTT_INTEGRATION_HOST:$MQTT_INTEGRATION_PORT"
        echo "Websocket Integration: $WEBSOCKET_INTEGRATION_URL ($WEBSOCKET_INTEGRATION_HOST)"
        echo
        bold "========================================================"
        bold "  Examples"
        bold "========================================================"
        echo
        echo "Grafana:          $DASHBOARD_URL"
        echo
    } >&3

fi
