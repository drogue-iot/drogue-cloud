#!/usr/bin/env bash

# Dump out the dashboard URL and sample commands for http and mqtt
set -x
: "${CLUSTER:=minikube}"
: "${DROGUE_NS:=drogue-iot}"
: "${CONSOLE:=true}"
: "${MQTT:=true}"

case $CLUSTER in
   minikube)
        CONSOLE_URL=$(eval minikube service -n $DROGUE_NS --url console-frontend)
        DASHBOARD_URL=$(eval minikube service -n $DROGUE_NS --url grafana)
        ;;
   *)
        CONSOLE_URL=$(eval kubectl -n $DROGUE_NS get routes console -o jsonpath={.spec.host})
        DASHBOARD_URL=$(eval kubectl -n $DROGUE_NS get routes grafana -o jsonpath={.spec.host})
        ;;
esac;


# Dump out the dashboard URL and sample commands for http and mqtt
set +x
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
