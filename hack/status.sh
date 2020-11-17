#!/usr/bin/env bash

# Dump out the dashboard URL and sample commands for http and mqtt
set +x

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
source "$SCRIPTDIR/common.sh"

: "${CLUSTER:=minikube}"
: "${PLATFORM:=kubernetes}"
: "${CONSOLE:=true}"
: "${MQTT:=true}"
: "${DIGITAL_TWIN:=false}"

HTTP_ENDPOINT_URL=$(eval "kubectl get ksvc -n $DROGUE_NS http-endpoint -o jsonpath='{.status.url}'")

case $CLUSTER in
    kind)
        DOMAIN=$(kubectl get node kind-control-plane -o jsonpath='{.status.addresses[?(@.type == "InternalIP")].address}').nip.io
        CONSOLE_PORT=$(kubectl get service -n $DROGUE_NS console-frontend -o jsonpath='{.spec.ports[0].nodePort}')
        GRAFANA_PORT=$(kubectl get service -n $DROGUE_NS grafana -o jsonpath='{.spec.ports[0].nodePort}')

        CONSOLE_URL=http://console-frontend.$DOMAIN:$CONSOLE_PORT
        DASHBOARD_URL=http://grafana.$DOMAIN:$GRAFANA_PORT
        ;;
   minikube)
        MQTT_ENDPOINT_HOST=$(eval minikube service -n $DROGUE_NS --url mqtt-endpoint | awk -F[/:] '{print $4 ".nip.io"}')
        MQTT_ENDPOINT_PORT=$(eval minikube service -n $DROGUE_NS --url mqtt-endpoint | awk -F[/:] '{print $5}')
        CONSOLE_URL=$(eval minikube service -n $DROGUE_NS --url console-frontend)
        DASHBOARD_URL=$(eval minikube service -n $DROGUE_NS --url grafana)
        ;;
   *)
        MQTT_ENDPOINT_HOST=$(eval kubectl get route -n drogue-iot mqtt-endpoint -o jsonpath='{.status.ingress[0].host}')
        MQTT_ENDPOINT_PORT=443
        CONSOLE_URL=$(eval kubectl -n $DROGUE_NS get routes console -o jsonpath={.spec.host})
        DASHBOARD_URL=$(eval kubectl -n $DROGUE_NS get routes grafana -o jsonpath={.spec.host})
        ;;
esac;


# Dump out the dashboard URL and sample commands for http and mqtt

echo ""
if [ $CONSOLE = "true" ] ; then
  echo "Console:"
  echo "  $CONSOLE_URL"
  echo ""
fi
echo "Login to Grafana:"
echo "  url:      $DASHBOARD_URL"
echo "  username: admin"
echo "  password: admin123456"
echo "Search for the 'Knative test' dashboard"
echo ""
echo "At a shell prompt, try these commands:"
echo "  http POST $HTTP_ENDPOINT_URL/publish/device_id/foo temp:=44"
if [ "$MQTT" = true ] ; then
  echo "  mqtt pub -v -h $MQTT_ENDPOINT_HOST -p $MQTT_ENDPOINT_PORT -s --cafile tls.crt -t temp -m '{\"temp\":42}' -V 3"
fi

#
# expects "VAR=value" as an argument, which gets printed and executed.
#
function setexec() {
  echo "$@"
  # shellcheck disable=SC2163
  export "$@"
}

if [[ "$DIGITAL_TWIN" == "true" ]]; then

echo
echo "=========================================================================================="
echo " Digital Twin:"
echo "=========================================================================================="
echo

setexec ENDPOINT="$(kubectl get ksvc -n "$DROGUE_NS" http-endpoint -o jsonpath='{.status.url}')"

case $PLATFORM in
openshift)
setexec TWIN_API="https://ditto:ditto@$(kubectl -n "$DROGUE_NS" get route ditto-console -o jsonpath='{.spec.host}')"
  ;;
*)
setexec TWIN_API="http://ditto:ditto@$(kubectl -n "$DROGUE_NS" get ingress ditto -o jsonpath='{.status.loadBalancer.ingress[0].ip}' 2>/dev/null)"
  ;;
esac

setexec DEVICE_ID="my:dev1"
setexec CHANNEL="foo"
setexec MODEL_ID="io.drogue.demo:FirstTestDevice:1.0.0"

echo

echo "------------------------------------------------------------------------------------------"
echo "Examples"
echo "------------------------------------------------------------------------------------------"
echo
echo "Fetch the model:"
echo "-------------------"
echo
echo "http -do FirstTestDevice.json https://vorto.eclipse.org/api/v1/generators/eclipseditto/models/$MODEL_ID/?target=thingJson"
echo
echo "Create a new device:"
echo "-----------------------"
echo
echo "cat FirstTestDevice.json | http PUT \"$TWIN_API/api/2/things/$DEVICE_ID\""
echo
echo "Publish some data:"
echo "-----------------------"
echo
echo "http -v POST \"$ENDPOINT/publish/$DEVICE_ID/$CHANNEL\" \"model_id=="$MODEL_ID"\" temp:=1.23"
echo
echo "Check the twin status:"
echo "-----------------------"
echo
echo "http \"$TWIN_API/api/2/things/$DEVICE_ID\""
echo

fi