#!/usr/bin/env bash

set -ex

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
DEPLOY_DIR="$(dirname "${BASH_SOURCE[0]}")/../deploy/02-deploy"
CLUSTER="openshift"
MQTT=false
CONSOLE=false
HELM_ARGS="--values $SCRIPTDIR/../deploy/helm/drogue-iot/profile-openshift.yaml"

source "$SCRIPTDIR/common.sh"
source "$SCRIPTDIR/knative.sh"
source "$SCRIPTDIR/postgres.sh"

# Create workspace for endpoints
if ! kubectl get ns $DROGUE_NS >/dev/null 2>&1; then
  kubectl create namespace $DROGUE_NS
  kubectl label namespace $DROGUE_NS bindings.knative.dev/include=true
fi

# Create kafka cluster
kubectl apply -f $DEPLOY_DIR/01-kafka/010-Kafka.yaml
kubectl patch kafka -n knative-eventing kafka-eventing -p '[{"op": "remove", "path": "/spec/kafka/listeners/external"}]' --type json
kubectl wait kafka --all --for=condition=Ready --timeout=-1s -n knative-eventing

if [ "$MQTT" = true ] ; then
  HELM_ARGS+=" --set sources.mqtt.enabled=true"
fi

if [ "$CONSOLE" = true ] ; then
  HELM_ARGS+=" --set services.console.enabled=true"
fi

helm install --dependency-update -n $DROGUE_NS $HELM_ARGS drogue-iot $SCRIPTDIR/../deploy/helm/drogue-iot/

# Provide a TLS certificate for the MQTT endpoint
if  [ "$MQTT" = true ] && ! [[kubectl -n $DROGUE_NS get secret mqtt-endpoint-tls >/dev/null 2>&1]] ; then
  openssl req -x509 -nodes -days 365 -newkey rsa:2048 -keyout tls_tmp.key -out tls.crt -subj "/CN=foo.bar.com" -addext "subjectAltName = DNS:$(kubectl get route -n drogue-iot mqtt-endpoint -o jsonpath='{.status.ingress[0].host}')"
  openssl rsa -in tls_tmp.key -out tls.key
  kubectl -n $DROGUE_NS create secret tls mqtt-endpoint-tls --key tls.key --cert tls.crt
fi

# Wait for the HTTP endpoint to become ready

kubectl -n $DROGUE_NS wait --timeout=-1s --for=condition=Ready ksvc/http-endpoint

# Create the Console endpoints
if [ $CONSOLE = "true" ] ; then
  kubectl -n $DROGUE_NS set env deployment/console-backend "ENDPOINT_SOURCE-"
  kubectl -n $DROGUE_NS set env deployment/console-backend "HTTP_ENDPOINT_URL=$(kubectl get ksvc -n $DROGUE_NS http-endpoint -o jsonpath='{.status.url}')"

  kubectl -n $DROGUE_NS set env deployment/console-backend "MQTT_ENDPOINT_HOST=$(kubectl get route -n drogue-iot mqtt-endpoint -o jsonpath='{.status.ingress[0].host}')"
  kubectl -n $DROGUE_NS set env deployment/console-backend "MQTT_ENDPOINT_PORT=443"

  kubectl -n $DROGUE_NS set env deployment/console-frontend "BACKEND_URL=https://$(kubectl get route -n $DROGUE_NS console-backend -o 'jsonpath={ .spec.host }')" "CLUSTER_DOMAIN-"
fi


kubectl wait ksvc --all --timeout=-1s --for=condition=Ready -n $DROGUE_NS
kubectl wait deployment --all --timeout=-1s --for=condition=Available -n $DROGUE_NS


# Dump out the dashboard URL and sample commands for http and mqtt
set +x
echo ""
if [ $CONSOLE = "true" ] ; then
  echo "Console:"
  echo "  $(kubectl -n $DROGUE_NS get routes console -o jsonpath={.spec.host})"
  echo ""
fi
echo "Login to Grafana:"
echo "  url:      $(kubectl -n $DROGUE_NS get routes grafana -o jsonpath={.spec.host})"
echo "  username: admin"
echo "  password: admin123456"
echo "Search for the 'Knative test' dashboard"
echo ""
echo "At a shell prompt, try these commands:"
echo "  http POST $(kubectl get ksvc -n $DROGUE_NS http-endpoint -o jsonpath='{.status.url}')/publish/device_id/foo temp:=44"
if [ "$MQTT" = true ] ; then
  echo "  mqtt pub -v -h $(kubectl get route -n drogue-iot mqtt-endpoint -o jsonpath='{.status.ingress[0].host}') -p 443 -s --cafile tls.crt -t temp -m '{\"temp\":42}' -V 3"
fi
