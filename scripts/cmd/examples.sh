#!/usr/bin/env bash

set -e

# Dump out the dashboard URL and sample commands for http and mqtt

: "${DIGITAL_TWIN:=false}"
: "${EXAMPLES:=true}"

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

if [[ "$METRICS" == "true" ]]; then
echo
bold "View the metrics dashboard:"
bold "----------------------------"
echo
echo "* Login to Grafana: https://$METRICS_DASHBOARD_URL"
echo "* Default credentials are 'admin/admin123456' if not configured differently"
echo
fi

bold "------------------------------------------------------------------------------------------"
bold "Examples"
bold "------------------------------------------------------------------------------------------"

if [[ "$EXAMPLES" == "true" ]]; then
echo
bold "View the example dashboard (if it's installed):"
bold "----------------------------"
echo
echo "* Login to Grafana (using SSO): https://$EXAMPLES_DASHBOARD_URL"
echo "* You will be presented with the 'Temperatures dashboard'"
fi

if test -f build/certs/endpoints/root-cert.pem; then
    HTTP_VERIFY="--verify build/certs/endpoints/root-cert.pem "
    MQTT_VERIFY="--cafile build/certs/endpoints/root-cert.pem "
fi

echo
bold "Login with 'drg':"
bold "---------------------"
echo
echo "* Get drg: https://github.com/drogue-iot/drg/releases/latest"
echo "* Execute:"
echo "  drg login $API_URL"
echo
bold "Initial application and device:"
bold "--------------------------------"
echo
echo "The installer created a default application 'example-app' and device 'device1' for you. The following commands were used:"
echo
echo "  drg create app example-app"
echo "  drg create device --app example-app device1 --spec '{\"credentials\": {\"credentials\":[{ \"pass\": \"hey-rodney\" }]}}'"
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
echo "Or simply through drg:"
echo "  drg stream example-app"
echo
bold "Publish data:"
bold "---------------"
echo
echo "After you created a device, try these commands at a shell prompt:"
echo
echo "  http --auth device1@example-app:hey-rodney ${HTTP_VERIFY}POST $HTTP_ENDPOINT_URL/v1/foo temp:=42"
echo "  mqtt pub -v -h $MQTT_ENDPOINT_HOST -p $MQTT_ENDPOINT_PORT -u device1@example-app -pw hey-rodney -s ${MQTT_VERIFY}-t temp -m '{\"temp\":42}'"
echo "  mqtt pub -v -ws -h $MQTT_ENDPOINT_WS_HOST -p $MQTT_ENDPOINT_WS_PORT -u device1@example-app -pw hey-rodney -s ${MQTT_VERIFY}-t temp -m '{\"temp\":42}'"
echo
bold "Send commands to the device:"
bold "------------------------------"
echo
echo "Publish data from the device and specify how long will you wait for a command with 'ct' parameter (in seconds):"
echo
if test -f build/certs/endpoints/root-cert.pem; then
  echo "  http --auth device1@example-app:hey-rodney --verify build/certs/endpoints/root-cert.pem POST $HTTP_ENDPOINT_URL/v1/foo?ct=30 temp:=42"
else
  echo "  http --auth device1@example-app:hey-rodney POST $HTTP_ENDPOINT_URL/v1/foo?ct=30 temp:=42"
fi
echo
echo "Or, subscribe with the MQTT device:"
echo
echo "  mqtt sub -v -h $MQTT_ENDPOINT_HOST -p $MQTT_ENDPOINT_PORT -u device1@example-app -pw hey-rodney -i device1 -s ${MQTT_VERIFY}-t command/inbox/#"
echo "  mqtt sub -v -ws -h $MQTT_ENDPOINT_WS_HOST -p $MQTT_ENDPOINT_WS_PORT -u device1@example-app -pw hey-rodney -i device1 -s ${MQTT_VERIFY}-t command/inbox/#"
echo
echo "Then, send a command to that device from another terminal window:"
echo
echo "  http POST $API_URL/api/command/v1alpha1/apps/example-app/devices/device1 command==set-temp target-temp:=25" \"Authorization:Bearer \$\(drg whoami -t\)\"
echo
echo "Or simply through drg:"
echo
echo "  drg cmd set-temp device1 --app example-app --payload '{\"target-temp\":25}' "
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
echo "  drg create device --app example-app device1 --spec '{\"credentials\": {\"credentials\":[{ \"pass\": \"hey-rodney\" }]}}'"
echo "  cat FirstTestDevice.json | http --auth ditto:ditto PUT $TWIN_API/api/2/things/example-app:device1"
echo
echo "Publish some data:"
echo "-----------------------"
echo
echo "System default certificates (or none):"
echo
echo "  http --auth device1@example-app:hey-rodney POST $HTTP_ENDPOINT_URL/v1/foo data_schema==vorto:io.drogue.demo:FirstTestDevice:1.0.0 temp:=24"
echo
echo "Local test certificates:"
echo
echo "  http --auth device1@example-app:hey-rodney --verify build/certs/endpoints/root-cert.pem POST $HTTP_ENDPOINT_URL/v1/foo data_schema==vorto:io.drogue.demo:FirstTestDevice:1.0.0 temp:=24"
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
