#!/usr/bin/env bash

set -ex

DROGUE_NS=${DROGUE_NS:-drogue-iot}
DEPLOY_DIR="$(dirname "${BASH_SOURCE[0]}")/../deploy/02-deploy"
SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
CLUSTER="ocp"

source "$SCRIPTDIR/knative.sh"

# Create workspace for endpoints
if ! kubectl get ns $DROGUE_NS >/dev/null 2>&1; then
  kubectl create namespace $DROGUE_NS
  kubectl label namespace $DROGUE_NS bindings.knative.dev/include=true
fi

# Create kafka cluster
kubectl apply -f $DEPLOY_DIR/01-kafka/010-Kafka.yaml
kubectl patch kafka -n knative-eventing kafka-eventing -p '[{"op": "remove", "path": "/spec/kafka/listeners/external"}]' --type json
kubectl wait kafka --all --for=condition=Ready --timeout=-1s -n knative-eventing

# Create InfluxDB
kubectl -n $DROGUE_NS apply -f $DEPLOY_DIR/02-influxdb

# Create Grafana dashboard
kubectl -n $DROGUE_NS apply -f $DEPLOY_DIR/03-dashboard

# Create needed knative resources
kubectl -n $DROGUE_NS apply -f $DEPLOY_DIR/04-knative

# Create the http endpoint
kubectl -n $DROGUE_NS apply -f $DEPLOY_DIR/05-endpoints/http

# Create the MQTT endpoint
kubectl -n $DROGUE_NS apply -f $DEPLOY_DIR/05-endpoints/mqtt

# Provide a TLS certificate for the MQTT endpoint
if ! kubectl -n $DROGUE_NS get secret mqtt-endpoint-tls >/dev/null 2>&1; then
  openssl req -x509 -nodes -days 365 -newkey rsa:2048 -keyout tls_tmp.key -out tls.crt -subj "/CN=foo.bar.com" -addext "subjectAltName = DNS:$(kubectl get route -n drogue-iot mqtt-endpoint -o jsonpath='{.status.ingress[0].host}')"
  openssl rsa -in tls_tmp.key -out tls.key
  kubectl -n $DROGUE_NS create secret tls mqtt-endpoint-tls --key tls.key --cert tls.crt
fi

# Create the Console endpoints
kubectl -n $DROGUE_NS apply -f $DEPLOY_DIR/06-console
kubectl -n $DROGUE_NS apply -f $DEPLOY_DIR/06-console/ocp

kubectl -n $DROGUE_NS set env deployment/console-backend "ENDPOINT_SOURCE-"
kubectl -n $DROGUE_NS set env deployment/console-backend "HTTP_ENDPOINT_URL=$(kubectl get ksvc -n $DROGUE_NS http-endpoint -o jsonpath='{.status.url}')"
kubectl -n $DROGUE_NS set env deployment/console-frontend "BACKEND_URL=https://$(oc get route -n $DROGUE_NS console-backend -o 'jsonpath={ .spec.host }')" "CLUSTER_DOMAIN-"

kubectl wait ksvc --all --timeout=-1s --for=condition=Ready -n $DROGUE_NS
kubectl wait deployment --all --timeout=-1s --for=condition=Available -n $DROGUE_NS


# Dump out the dashboard URL and sample commands for http and mqtt
set +x
echo ""
echo "Console:"
echo "  $(kubectl -n $DROGUE_NS get routes console -o jsonpath={.spec.host})"
echo ""
echo "Login to Grafana:"
echo "  url:      $(kubectl -n $DROGUE_NS get routes grafana -o jsonpath={.spec.host})"
echo "  username: admin"
echo "  password: admin123456"
echo "Search for the 'Knative test' dashboard"
echo ""
echo "At a shell prompt, try these commands:"
echo "  http POST $(kubectl get ksvc -n $DROGUE_NS http-endpoint -o jsonpath='{.status.url}')/publish/foo temp:=44"
echo "  mqtt pub -v -h $(kubectl get route -n drogue-iot mqtt-endpoint -o jsonpath='{.status.ingress[0].host}') -p 443 -s --cafile tls.crt -t temp -m '{\"temp\":42}' -V 3"
