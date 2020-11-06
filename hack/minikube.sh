#!/usr/bin/env bash

set -ex

DROGUE_NS=${DROGUE_NS:-drogue-iot}
DEPLOY_DIR="$(dirname "${BASH_SOURCE[0]}")/../deploy/02-deploy"
SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
CLUSTER="minikube"

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
for f in $(ls $DEPLOY_DIR/03-dashboard); do
  if [[ ! $f =~ 'Route' ]]; then
    kubectl -n $DROGUE_NS apply -f $DEPLOY_DIR/03-dashboard/$f
  fi
done
kubectl patch svc -n $DROGUE_NS grafana -p "{\"spec\": {\"type\": \"NodePort\"}}"

# Create needed knative resources
kubectl -n $DROGUE_NS apply -f $DEPLOY_DIR/04-knative

# Create the http endpoint
kubectl -n $DROGUE_NS apply -f $DEPLOY_DIR/05-endpoints/http

# Create the mqtt endpoint
for f in $(ls $DEPLOY_DIR/05-endpoints/mqtt); do
  if [[ ! $f =~ 'Route' ]]; then
    kubectl -n $DROGUE_NS apply -f $DEPLOY_DIR/05-endpoints/mqtt/$f
  fi
done
kubectl -n $DROGUE_NS patch svc mqtt-endpoint -p "{\"spec\": {\"type\": \"NodePort\"}}"

# Provide a TLS certificate for the MQTT endpoint
if ! kubectl -n $DROGUE_NS get secret mqtt-endpoint-tls >/dev/null 2>&1; then
  openssl req -x509 -nodes -days 365 -newkey rsa:2048 -keyout tls_tmp.key -out tls.crt -subj "/CN=foo.bar.com" -addext $(minikube service -n $DROGUE_NS --url mqtt-endpoint | awk -F[/:] '{print "subjectAltName=DNS:" $4 ".nip.io"}')
  openssl rsa -in tls_tmp.key -out tls.key
  kubectl -n $DROGUE_NS create secret tls mqtt-endpoint-tls --key tls.key --cert tls.crt
fi

# Wait for the HTTP endpoint to become ready

kubectl -n $DROGUE_NS wait --timeout=-1s --for=condition=Ready ksvc/http-endpoint

# Deploy the console
kubectl -n $DROGUE_NS apply -f $DEPLOY_DIR/06-console
kubectl -n $DROGUE_NS patch svc console-backend -p "{\"spec\": {\"type\": \"NodePort\"}}"
kubectl -n $DROGUE_NS patch svc console-frontend -p "{\"spec\": {\"type\": \"NodePort\"}}"
kubectl -n $DROGUE_NS set env deployment/console-frontend "BACKEND_URL=$(minikube service -n $DROGUE_NS --url console-backend)" "CLUSTER_DOMAIN-"
kubectl -n $DROGUE_NS set env deployment/console-backend "ENDPOINT_SOURCE-"
kubectl -n $DROGUE_NS set env deployment/console-backend "HTTP_ENDPOINT_URL=$(kubectl get ksvc -n $DROGUE_NS http-endpoint -o jsonpath='{.status.url}')"
kubectl -n $DROGUE_NS set env deployment/console-backend "MQTT_ENDPOINT_HOST=$(minikube service -n $DROGUE_NS --url mqtt-endpoint | awk -F[/:] '{print $4 ".nip.io"}')"
kubectl -n $DROGUE_NS set env deployment/console-backend "MQTT_ENDPOINT_PORT=$(minikube service -n $DROGUE_NS --url mqtt-endpoint | awk -F[/:] '{print $5}')"

kubectl wait ksvc --all --timeout=-1s --for=condition=Ready -n $DROGUE_NS
#kubectl -n $DROGUE_NS set env deploy/mqtt-endpoint DISABLE_TLS=true
kubectl wait deployment --all --timeout=-1s --for=condition=Available -n $DROGUE_NS

# Dump out the dashboard URL and sample commands for http and mqtt
set +x
echo ""
echo "Console:"
echo "  $(minikube service -n $DROGUE_NS --url console-frontend)"
echo ""
echo "Login to Grafana:"
echo "  url:      $(minikube service -n $DROGUE_NS --url grafana)"
echo "  username: admin"
echo "  password: admin123456"
echo "Search for the 'Knative test' dashboard"
echo ""
echo "At a shell prompt, try these commands:"
echo "  http POST $(kubectl get ksvc -n $DROGUE_NS http-endpoint -o jsonpath='{.status.url}')/publish/device_id/foo temp:=44"
minikube service -n $DROGUE_NS --url mqtt-endpoint | awk -F[/:] '{print "  mqtt pub -v -h " $4 ".nip.io -p " $5 " -s --cafile tls.crt -t temp -m '\''{\"temp\":42}'\'' -V 3"}'
