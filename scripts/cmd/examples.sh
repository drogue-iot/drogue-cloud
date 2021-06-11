#!/usr/bin/env bash

set -e

# Dump out the dashboard URL and sample commands for http and mqtt

: "${DIGITAL_TWIN:=false}"

SILENT=true source "${SCRIPTDIR}/cmd/__endpoints.sh"

# group the following out so that we can send it to fd3
{

# Dump out the dashboard URL and sample commands for http and mqtt

echo
bold "=========================================================================================="
bold " Base"
bold "=========================================================================================="
echo
bold "Single sign-on:"
echo "  URL:      $SSO_URL"
echo "  User:     admin"
echo "  Password: admin123456"
echo
echo "$(bold -n Console:) $CONSOLE_URL"
echo
bold "------------------------------------------------------------------------------------------"
bold "Examples"
bold "------------------------------------------------------------------------------------------"
echo
bold "View the example dashboard:"
bold "----------------------------"
echo
echo "* Login to Grafana (using SSO): $DASHBOARD_URL"
echo "* Search for the 'Examples' dashboard"
echo
bold "Login in with 'drg':"
bold "---------------------"
echo
echo "  drg login $API_URL"
echo
bold "Create an initial application and device:"
bold "------------------------------"
echo
echo "  drg create app app_id"
echo "  drg create device --app app_id device_id --data '{\"credentials\": {\"credentials\":[{ \"pass\": \"foobar\" }]}}'"
echo
bold "Subscribe to device data:"
bold "---------------------------"
echo
echo "Data published by devices can be received via MQTT. Possibly start this in another terminal."
echo
echo "Structured content mode (MQTT v3.1.1 and v5):"
echo "  mqtt sub -v -h $MQTT_INTEGRATION_HOST -p $MQTT_INTEGRATION_PORT -pw \"\$(drg token)\" -s --cafile build/certs/endpoints/ca-bundle.pem -t 'app/app_id'"
echo
echo "Binary content mode (MQTT v5 only):"
echo "  mqtt sub -v -h $MQTT_INTEGRATION_HOST -p $MQTT_INTEGRATION_PORT -pw \"\$(drg token)\" -s --cafile build/certs/endpoints/ca-bundle.pem -t 'app/app_id'" -up content-mode=binary
echo
bold "Publish data:"
bold "---------------"
echo
echo "After you created a device, try these commands at a shell prompt:"
echo
if test -f build/certs/endpoints/ca-bundle.pem; then
  echo "  http --auth device_id@app_id:foobar --verify build/certs/endpoints/ca-bundle.pem POST $HTTP_ENDPOINT_URL/v1/foo temp:=42"
  echo "  mqtt pub -v -h $MQTT_ENDPOINT_HOST -p $MQTT_ENDPOINT_PORT -u device_id@app_id -pw foobar -s --cafile build/certs/endpoints/ca-bundle.pem -t temp -m '{\"temp\":42}'"
else
  echo "  http --auth device_id@app_id:foobar POST $HTTP_ENDPOINT_URL/v1/foo temp:=42"
  echo "  mqtt pub -v -h $MQTT_ENDPOINT_HOST -p $MQTT_ENDPOINT_PORT -u device_id@app_id -pw foobar -s -t temp -m '{\"temp\":42}'"
fi
echo
bold "Send commands to the device:"
bold "------------------------------"
echo
echo "Publish data from the device and specify how long will you wait for a command with 'ct' parameter (in seconds):"
echo
if test -f build/certs/endpoints/ca-bundle.pem; then
  echo "  http --auth device_id@app_id:foobar --verify build/certs/endpoints/ca-bundle.pem POST $HTTP_ENDPOINT_URL/v1/foo?ct=30 temp:=42"
else
  echo "  http --auth device_id@app_id:foobar POST $HTTP_ENDPOINT_URL/v1/foo?ct=30 temp:=42"
fi
echo
echo "Or, subscribe with the MQTT device:"
echo
if test -f build/certs/endpoints/ca-bundle.pem; then
  echo "  mqtt sub -v -h $MQTT_ENDPOINT_HOST -p $MQTT_ENDPOINT_PORT -u device_id@app_id -pw foobar -i device_id -s --cafile build/certs/endpoints/ca-bundle.pem -t command/inbox"
else
  echo "  mqtt sub -v -h $MQTT_ENDPOINT_HOST -p $MQTT_ENDPOINT_PORT -u device_id@app_id -pw foobar -i device_id -s -t command/inbox"
fi
echo
echo "Then, send a command to that device from another terminal window:"
echo
echo "  http POST $COMMAND_ENDPOINT_URL/command application==app_id device==device_id command==set-temp target-temp:=25" \"Authorization:Bearer \$\(drg token\)\"
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

if [[ "$CLUSTER" = "minikube" ]] ; then
  TWIN_API=$(eval minikube service -n "$DROGUE_NS" --url ditto-nginx-external)
else
  TWIN_API=$(ingress_url "ditto")
fi

echo
echo "=========================================================================================="
echo " Digital Twin:"
echo "=========================================================================================="
echo
echo "Twin API: $TWIN_API"
echo
echo "------------------------------------------------------------------------------------------"
echo "Examples"
echo "------------------------------------------------------------------------------------------"
echo
echo "Fetch the model:"
echo "-------------------"
echo
echo "  http -do FirstTestDevice.json https://vorto.eclipseprojects.io/api/v1/generators/eclipseditto/models/io.drogue.demo:FirstTestDevice:1.0.0/?target=thingJson"
echo
echo "Create a new device:"
echo "-----------------------"
echo
echo "  drg create app app_id"
echo "  drg create device --app app_id device_id --data '{\"credentials\": {\"credentials\":[{ \"pass\": \"foobar\" }]}}'"
echo "  cat FirstTestDevice.json | http --auth ditto:ditto PUT $TWIN_API/api/2/things/app_id:device_id"
echo
echo "Publish some data:"
echo "-----------------------"
echo
echo "System default certificates (or none):"
echo
echo "  http --auth device_id@app_id:foobar POST $HTTP_ENDPOINT_URL/v1/foo data_schema==vorto:io.drogue.demo:FirstTestDevice:1.0.0 temp:=24"
echo
echo "Local test certificates:"
echo
echo "  http --auth device_id@app_id:foobar --verify build/certs/endpoints/ca-bundle.pem POST $HTTP_ENDPOINT_URL/v1/foo data_schema==vorto:io.drogue.demo:FirstTestDevice:1.0.0 temp:=24"
echo
echo "Check the twin status:"
echo "-----------------------"
echo
echo "  http --auth ditto:ditto $TWIN_API/api/2/things/app_id:device_id"
echo

fi

echo "------------------------------------------------------------------------------------------"
echo
echo "You can view this information again by executing the following command:"
echo
if is_default_cluster; then
echo "    $SCRIPTDIR/drgadm status"
else
echo "    env CLUSTER=$CLUSTER $SCRIPTDIR/drgadm status"
fi

echo

} >&3
