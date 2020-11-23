#!/usr/bin/env bash

# Dump out the dashboard URL and sample commands for http and mqtt
set +x

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
source "$SCRIPTDIR/common.sh"

: "${CLUSTER:=minikube}"
: "${CONSOLE:=true}"
: "${MQTT:=true}"
: "${DIGITAL_TWIN:=false}"

HTTP_ENDPOINT_URL=$(eval "kubectl get ksvc -n $DROGUE_NS http-endpoint -o jsonpath='{.status.url}'")

case $CLUSTER in
    kind)
        ;;
   minikube)
        MQTT_ENDPOINT_HOST=$(eval minikube service -n "$DROGUE_NS" --url mqtt-endpoint | awk -F[/:] '{print $4 ".nip.io"}')
        MQTT_ENDPOINT_PORT=$(eval minikube service -n "$DROGUE_NS" --url mqtt-endpoint | awk -F[/:] '{print $5}')
        ;;
   openshift)
        MQTT_ENDPOINT_HOST=$(eval kubectl get route -n "$DROGUE_NS" mqtt-endpoint -o jsonpath='{.status.ingress[0].host}')
        MQTT_ENDPOINT_PORT=443
        HTTP_ENDPOINT_URL=$(kubectl get ksvc -n "$DROGUE_NS" http-endpoint -o jsonpath='{.status.url}' | sed 's/http:/https:/')
        ;;
   *)
        echo "Unknown Kubernetes platform: $CLUSTER ... unable to extract endpoints"
        exit 1
        ;;
esac;

CONSOLE_URL=$(service_url "console")
DASHBOARD_URL=$(service_url "grafana")
SSO_URL=$(ingress_url "keycloak")

# Dump out the dashboard URL and sample commands for http and mqtt

echo
echo "=========================================================================================="
echo " Base:"
echo "=========================================================================================="
echo

echo "SSO:"
echo "  url:      $SSO_URL"
echo "  user:     admin"
echo "  password: admin123456"
echo

if [ $CONSOLE = "true" ] ; then
  echo "Console:"
  echo "  url:      $CONSOLE_URL"
  echo "  user:     admin"
  echo "  password: admin123456"
  echo
fi

echo "------------------------------------------------------------------------------------------"
echo "Examples"
echo "------------------------------------------------------------------------------------------"
echo
echo "View the dashboard:"
echo "---------------------"
echo
echo "* Login to Grafana:"
echo "    url:      $DASHBOARD_URL"
echo "    username: admin"
echo "    password: admin123456"
echo "* Try this link: $DASHBOARD_URL/d/YYGTNzdMk/"
echo "* Or search for the 'Knative test' dashboard"
echo
echo "Publish data:"
echo "----------------"
echo
echo "At a shell prompt, try these commands:"
echo
echo "  http POST $HTTP_ENDPOINT_URL/publish/device_id/foo temp:=44"
if [ "$MQTT" = true ] ; then
  echo "  mqtt pub -v -h $MQTT_ENDPOINT_HOST -p $MQTT_ENDPOINT_PORT -s --cafile tls.crt -t temp -m '{\"temp\":42}' -V 3"
fi
echo

#
# Expects "VAR=value" as an argument, which gets printed and executed.
#
# This is used to show an env-var command to the user, and make the same value available in the script later on.
#
function sete() {
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

sete ENDPOINT="$(kubectl get ksvc -n "$DROGUE_NS" http-endpoint -o jsonpath='{.status.url}')"

case $CLUSTER in
openshift)
sete TWIN_API="https://ditto:ditto@$(kubectl -n "$DROGUE_NS" get route ditto-console -o jsonpath='{.spec.host}')"
  ;;
*)
sete TWIN_API="http://ditto:ditto@$(kubectl -n "$DROGUE_NS" get ingress ditto -o jsonpath='{.status.loadBalancer.ingress[0].ip}' 2>/dev/null)"
  ;;
esac

sete DEVICE_ID="my:dev1"
sete CHANNEL="foo"
sete MODEL_ID="io.drogue.demo:FirstTestDevice:1.0.0"

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