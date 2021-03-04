#!/usr/bin/env bash

set -ex

: "${CLUSTER:="minikube"}"


SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
source "$SCRIPTDIR/../common.sh"

CERT_DIR=$SCRIPTDIR/../../device-management-service/tests/certs

MGMT_URL=$(service_url "registry")
HTTP_ENDPOINT_URL=$(service_url "http-endpoint" https)

case $CLUSTER in
    kind)
       DOMAIN=$(kubectl get node kind-control-plane -o jsonpath='{.status.addresses[?(@.type == "InternalIP")].address}').nip.io
       MQTT_ENDPOINT_HOST=mqtt-endpoint.$DOMAIN
       MQTT_ENDPOINT_PORT=$(kubectl get service -n "$DROGUE_NS" mqtt-endpoint -o jsonpath='{.spec.ports[0].nodePort}')
        ;;
   minikube)
        MQTT_ENDPOINT_HOST=$(eval minikube service -n "$DROGUE_NS" --url mqtt-endpoint | awk -F[/:] '{print $4 ".nip.io"}')
        MQTT_ENDPOINT_PORT=$(eval minikube service -n "$DROGUE_NS" --url mqtt-endpoint | awk -F[/:] '{print $5}')
        ;;
   openshift)
        MQTT_ENDPOINT_HOST=$(eval kubectl get route -n "$DROGUE_NS" mqtt-endpoint -o jsonpath='{.status.ingress[0].host}')
        MQTT_ENDPOINT_PORT=443
        ;;
   *)
        echo "Unknown Kubernetes platform: $CLUSTER ... unable to extract endpoints"
        exit 1
        ;;
esac;

CERT=$(base64 -w0 < "$CERT_DIR/trusted-certs.pem")

APP=cert1
DEVICE="O=Drogue IoT, OU=Cloud, CN=Device 1"
DEVICE_ENC="$(jq -rn --arg x "$DEVICE" '$x|@uri')"

http DELETE "$MGMT_URL/api/v1/apps/$APP" || true

http POST "$MGMT_URL/api/v1/apps" metadata:="{\"name\":\"$APP\"}" spec:="{\"trustAnchors\": {\"anchors\": [ {\"certificate\": \"$CERT\"} ]}}"
http POST "$MGMT_URL/api/v1/apps/$APP/devices" metadata:="{\"application\": \"$APP\", \"name\":\"$DEVICE\"}" spec:='{"credentials": {}}'

http GET "$MGMT_URL/api/v1/apps/$APP"
http GET "$MGMT_URL/api/v1/apps/$APP/devices/$DEVICE_ENC"

#mqtt pub -v -h "$MQTT_ENDPOINT_HOST" -p $MQTT_ENDPOINT_PORT -u device_id@app_id -pw foobar -s --cafile build/certs/endpoints/ca-bundle.pem -t temp -m '{\"temp\":42}'
http -v --cert "$CERT_DIR/device.1.pem" --verify "${SCRIPTDIR}/../../build/certs/endpoints/ca-bundle.pem" POST "$HTTP_ENDPOINT_URL/v1/foo" temp:=42
