#!/usr/bin/env bash

set -ex

: "${CLUSTER:="minikube"}"

BASEDIR="$(cd "$(dirname "${BASH_SOURCE[0]}")"/.. >/dev/null 2>&1 && pwd)"
source "$BASEDIR/lib/mod.sh"
SILENT=true source "$BASEDIR/cmd/__endpoints.sh"

CERT_DIR=$BASEDIR/../device-management-service/tests/certs

CERT=$(base64 -w0 < "$CERT_DIR/trusted-certs.pem")

APP=cert1
DEVICE="O=Drogue IoT, OU=Cloud, CN=Device 1"

drg delete app $APP || true

drg create app $APP --spec "{\"trustAnchors\": {\"anchors\": [ {\"certificate\": \"$CERT\"} ]}}"
drg create device --app $APP "$DEVICE" --spec '{"credentials": {}}'

drg get app $APP
drg get device --app $APP "$DEVICE"

#mqtt pub -v -h "$MQTT_ENDPOINT_HOST" -p $MQTT_ENDPOINT_PORT -u device_id@app_id -pw foobar -s --cafile build/certs/endpoints/ca-bundle.pem -t temp -m '{\"temp\":42}'
http -v --cert "$CERT_DIR/device.1.pem" --verify "${BASEDIR}/../build/certs/endpoints/ca-bundle.pem" POST "$HTTP_ENDPOINT_URL/v1/foo" temp:=42
