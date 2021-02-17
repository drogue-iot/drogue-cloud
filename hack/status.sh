#!/usr/bin/env bash

# Dump out the dashboard URL and sample commands for http and mqtt
set +x

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
source "$SCRIPTDIR/common.sh"

: "${CLUSTER:=minikube}"
: "${CONSOLE:=true}"
: "${DIGITAL_TWIN:=false}"

source "${SCRIPTDIR}/endpoints.sh"

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
echo "Admin user (via SSO):"
echo "  user:     admin"
echo "  password: admin123456"
echo

if [ $CONSOLE = "true" ] ; then
  echo "Console:"
  echo "  url:      $CONSOLE_URL"
  echo "  user:     <via sso>"
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
echo "    user:     <via sso>"
echo "* Try this link: $DASHBOARD_URL/d/YYGTNzdMk/"
echo "* Or search for the 'Knative test' dashboard"
echo "* Additional admin user:"
echo "    username: admin"
echo "    password: admin123456"
echo
echo "Manage applications/devices:"
echo "-------------------------"
echo
echo "URL:"
echo "    ${MGMT_URL}"
echo
echo "Applications:"
echo "  Create:  http POST   ${MGMT_URL}/api/v1/apps metadata:='{\"name\":\"app_id\"}'"
echo "  Read:    http GET    ${MGMT_URL}/api/v1/apps/app_id"
echo "  Update:  http PUT    ${MGMT_URL}/api/v1/apps/app_id metadata:='{\"name\":\"app_id\"}' spec:='{\"core\": {\"disabled\": true}}'"
echo "  Delete:  http DELETE ${MGMT_URL}/api/v1/apps/app_id"
echo
echo "Devices:"
echo "  Create:  http POST   ${MGMT_URL}/api/v1/apps/app_id/devices metadata:='{\"application\": \"app_id\", \"name\":\"device_id\"}' spec:='{\"credentials\": {\"credentials\":[{ \"pass\": \"foobar\" }]}}'"
echo "  Read:    http GET    ${MGMT_URL}/api/v1/apps/app_id/devices/device_id"
echo "  Update:  http PUT    ${MGMT_URL}/api/v1/apps/app_id/devices/device_id metadata:='{\"application\": \"app_id\", \"name\":\"device_id\"}' spec:='{\"credentials\": {\"credentials\":[{ \"pass\": \"foobar\" }]}}'"
echo "  Delete:  http DELETE ${MGMT_URL}/api/v1/apps/app_id/devices/device_id"
echo
echo "Publish data:"
echo "---------------"
echo
echo "After you created a device, try these commands at a shell prompt:"
echo
echo "System default certificates (or none):"
echo
echo "  http --auth device_id@app_id:foobar POST $HTTP_ENDPOINT_URL/v1/foo temp:=42"
echo "  mqtt pub -v -h $MQTT_ENDPOINT_HOST -p $MQTT_ENDPOINT_PORT -u device_id@app_id -pw foobar -s -t temp -m '{\"temp\":42}'"
echo
echo "Local test certificates:"
echo
echo "  http --auth device_id@app_id:foobar --verify build/certs/endpoints/ca-bundle.pem POST $HTTP_ENDPOINT_URL/v1/foo temp:=42"
echo "  mqtt pub -v -h $MQTT_ENDPOINT_HOST -p $MQTT_ENDPOINT_PORT -u device_id@app_id -pw foobar -s --cafile build/certs/endpoints/ca-bundle.pem -t temp -m '{\"temp\":42}'"
echo
echo "Send commands to the device:"
echo "------------------------------"
echo
echo "After you created a device, try these commands at a shell prompt:"
echo
echo "Publish data from the device and specify how long will you wait for a command with 'ttd' parameter (in seconds)"
echo
echo "  http --auth device_id@app_id:foobar POST $HTTP_ENDPOINT_URL/v1/foo?ttd=30 temp:=42"
echo "  http --auth device_id@app_id:foobar --verify build/certs/endpoints/ca-bundle.pem POST $HTTP_ENDPOINT_URL/v1/foo?ttd=30 temp:=42"
echo
echo "Or subscribe with the MQTT device"
echo
echo "  mqtt sub -v -h $MQTT_ENDPOINT_HOST -p $MQTT_ENDPOINT_PORT -u device_id@app_id -pw foobar -i device_id -s -t command"
echo "  mqtt sub -v -h $MQTT_ENDPOINT_HOST -p $MQTT_ENDPOINT_PORT -u device_id@app_id -pw foobar -i device_id -s --cafile build/certs/endpoints/ca-bundle.pem -t command"
echo
echo "Send command to that device from another terminal window:"
echo
echo "  http POST $COMMAND_ENDPOINT_URL/command/app_id/device_id/foo set-temp:=40"
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

echo "------------------------------------------------------------------------------------------"
echo
echo "You can view this information again by executing the following command:"
echo
echo "    env CLUSTER=$CLUSTER $SCRIPTDIR/status.sh"
echo
