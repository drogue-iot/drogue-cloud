#!/usr/bin/env bash

set -e

# Dump out the dashboard URL and sample commands for http and mqtt

: "${DIGITAL_TWIN:=false}"

SILENT=true source "${BASEDIR}/cmd/__endpoints.sh"

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
echo "  drg create app example-app"
echo "  drg create device --app example-app device1 --spec '{\"credentials\": {\"credentials\":[{ \"pass\": \"foobar\" }]}}'"
echo
bold "Subscribe to device data:"
bold "---------------------------"
echo
echo "Data published by devices can be received via MQTT. Possibly start this in another terminal."
echo
echo "Structured content mode (MQTT v3.1.1 and v5):"
echo "  mqtt sub -v -h $MQTT_INTEGRATION_HOST -p $MQTT_INTEGRATION_PORT -pw \"\$(drg whoami -t)\" -s --cafile build/certs/endpoints/root-cert.pem -t 'app/example-app'"
echo
echo "Binary content mode (MQTT v5 only):"
echo "  mqtt sub -v -h $MQTT_INTEGRATION_HOST -p $MQTT_INTEGRATION_PORT -pw \"\$(drg whoami -t)\" -s --cafile build/certs/endpoints/root-cert.pem -t 'app/example-app'" -up content-mode=binary
echo
echo "You can also subscribe to data using WebSockets, receiving Cloud Events:"
echo "  websocat  -H=\"Authorization: Bearer \$(drg whoami -t)\" $WEBSOCKET_INTEGRATION_URL/example-app"
echo
bold "Publish data:"
bold "---------------"
echo
echo "After you created a device, try these commands at a shell prompt:"
echo
if test -f build/certs/endpoints/root-cert.pem; then
  echo "  http --auth device1@example-app:foobar --verify build/certs/endpoints/root-cert.pem POST $HTTP_ENDPOINT_URL/v1/foo temp:=42"
  echo "  mqtt pub -v -h $MQTT_ENDPOINT_HOST -p $MQTT_ENDPOINT_PORT -u device1@example-app -pw foobar -s --cafile build/certs/endpoints/root-cert.pem -t temp -m '{\"temp\":42}'"
else
  echo "  http --auth device1@example-app:foobar POST $HTTP_ENDPOINT_URL/v1/foo temp:=42"
  echo "  mqtt pub -v -h $MQTT_ENDPOINT_HOST -p $MQTT_ENDPOINT_PORT -u device1@example-app -pw foobar -s -t temp -m '{\"temp\":42}'"
fi
echo
bold "Send commands to the device:"
bold "------------------------------"
echo
echo "Publish data from the device and specify how long will you wait for a command with 'ct' parameter (in seconds):"
echo
if test -f build/certs/endpoints/root-cert.pem; then
  echo "  http --auth device1@example-app:foobar --verify build/certs/endpoints/root-cert.pem POST $HTTP_ENDPOINT_URL/v1/foo?ct=30 temp:=42"
else
  echo "  http --auth device1@example-app:foobar POST $HTTP_ENDPOINT_URL/v1/foo?ct=30 temp:=42"
fi
echo
echo "Or, subscribe with the MQTT device:"
echo
if test -f build/certs/endpoints/root-cert.pem; then
  echo "  mqtt sub -v -h $MQTT_ENDPOINT_HOST -p $MQTT_ENDPOINT_PORT -u device1@example-app -pw foobar -i device1 -s --cafile build/certs/endpoints/root-cert.pem -t command/inbox/#"
else
  echo "  mqtt sub -v -h $MQTT_ENDPOINT_HOST -p $MQTT_ENDPOINT_PORT -u device1@example-app -pw foobar -i device1 -s -t command/inbox/#"
fi
echo
echo "Then, send a command to that device from another terminal window:"
echo
echo "  http POST $API_URL/api/command/v1alpha1/apps/example-app/devices/device1 command==set-temp target-temp:=25" \"Authorization:Bearer \$\(drg whoami -t\)\"
echo

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
echo "  drg create app example-app"
echo "  drg create device --app example-app device1 --spec '{\"credentials\": {\"credentials\":[{ \"pass\": \"foobar\" }]}}'"
echo "  cat FirstTestDevice.json | http --auth ditto:ditto PUT $TWIN_API/api/2/things/example-app:device1"
echo
echo "Publish some data:"
echo "-----------------------"
echo
echo "System default certificates (or none):"
echo
echo "  http --auth device1@example-app:foobar POST $HTTP_ENDPOINT_URL/v1/foo data_schema==vorto:io.drogue.demo:FirstTestDevice:1.0.0 temp:=24"
echo
echo "Local test certificates:"
echo
echo "  http --auth device1@example-app:foobar --verify build/certs/endpoints/root-cert.pem POST $HTTP_ENDPOINT_URL/v1/foo data_schema==vorto:io.drogue.demo:FirstTestDevice:1.0.0 temp:=24"
echo
echo "Check the twin status:"
echo "-----------------------"
echo
echo "  http --auth ditto:ditto $TWIN_API/api/2/things/example-app:device1"
echo

fi

echo "------------------------------------------------------------------------------------------"
echo
echo "You can view this information again by executing the following command:"
echo
if is_default_cluster; then
echo "    $BASEDIR/drgadm examples"
else
echo "    env CLUSTER=$CLUSTER $BASEDIR/drgadm examples"
fi

echo

} >&3
