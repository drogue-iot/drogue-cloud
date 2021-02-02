#!/usr/bin/env bash

: "${CLUSTER:=minikube}"
: "${MQTT:=true}"

: "${INSTALL_DEPS:=true}"
: "${INSTALL_KNATIVE:=${INSTALL_DEPS}}"
: "${INSTALL_KEYCLOAK_OPERATOR:=${INSTALL_DEPS}}"

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
source "$SCRIPTDIR/common.sh"
: "${DEPLOYDIR:=$(realpath "$SCRIPTDIR/../deploy")}"

# process arguments

help() {
cat << EOF
Usage: ./drogue.sh
Deploys Drogue IoT cloud

  -c, --cluster      The cluster type (default: $CLUSTER)
                     one of: minikube, kind, openshift
  -d, --directory    The base directory for the deployment scripts (default: $DEPLOYDIR)

EOF
}

opts=$(getopt --name "${BASH_SOURCE[0]}" -l "help,cluster:,directory:" -o "hc:d:" -- "$@")
eval set --$opts

while [[ $# -gt 0 ]]; do
  case "$1" in
    -c|--cluster)
      CLUSTER="$2"
      shift 2
      ;;
    -d|--directory)
      DEPLOYDIR="$2"
      shift 2
      ;;
    -h|--help)
      help
      exit 0
      ;;
    --)
      shift
      break
      ;;
    *)
      help
      exit 1
      ;;
  esac
done

set -ex

# Create the namespace first
if ! kubectl get ns "$DROGUE_NS" >/dev/null 2>&1; then
  kubectl create namespace "$DROGUE_NS"
  kubectl label namespace "$DROGUE_NS" bindings.knative.dev/include=true
fi

# install pre-reqs

[[ "$INSTALL_KNATIVE" == true ]] && source "$SCRIPTDIR/knative.sh"
[[ "$INSTALL_KEYCLOAK_OPERATOR" == true ]] && source "$SCRIPTDIR/sso.sh"

# Install Drogue components (sources and services)
kubectl -n "$DROGUE_NS" apply -k "$DEPLOYDIR/$CLUSTER/"
# Remove the unnecessary and wrong host entry for keycloak ingress

case $CLUSTER in
   openshift)
        wait_for_resource route/keycloak
        ;;
   *)
        wait_for_resource ingress/keycloak
        kubectl -n "$DROGUE_NS" patch ingress/keycloak --type json --patch '[{"op": "remove", "path": "/spec/rules/0/host"}]' || true
        ;;
esac;

CERT_ALTNAMES=""

case $CLUSTER in
   kind)
       DOMAIN=$(kubectl get node kind-control-plane -o jsonpath='{.status.addresses[?(@.type == "InternalIP")].address}').nip.io
       MQTT_ENDPOINT_HOST=mqtt-endpoint.$DOMAIN
       MQTT_ENDPOINT_PORT=$(kubectl get service -n "$DROGUE_NS" mqtt-endpoint -o jsonpath='{.spec.ports[0].nodePort}')
       HTTP_ENDPOINT_HOST=http-endpoint.$DOMAIN
       HTTP_ENDPOINT_PORT=$(kubectl get service -n "$DROGUE_NS" http-endpoint -o jsonpath='{.spec.ports[0].nodePort}')
       ;;
   minikube)
        MQTT_ENDPOINT_HOST=$(eval minikube service -n "$DROGUE_NS" --url mqtt-endpoint | awk -F[/:] '{print $4 ".nip.io"}')
        MQTT_ENDPOINT_PORT=$(eval minikube service -n "$DROGUE_NS" --url mqtt-endpoint | awk -F[/:] '{print $5}')
        HTTP_ENDPOINT_IP=$(eval minikube service -n "$DROGUE_NS" --url http-endpoint | awk -F[/:] '{print $4}')
        CERT_ALTNAMES="$CERT_ALTNAMES IP:$HTTP_ENDPOINT_IP, "
        HTTP_ENDPOINT_HOST=$(eval minikube service -n "$DROGUE_NS" --url http-endpoint | awk -F[/:] '{print $4 ".nip.io"}')
        HTTP_ENDPOINT_PORT=$(eval minikube service -n "$DROGUE_NS" --url http-endpoint | awk -F[/:] '{print $5}')
        ;;
   openshift)
        MQTT_ENDPOINT_HOST=$(eval kubectl get route -n drogue-iot mqtt-endpoint -o jsonpath='{.status.ingress[0].host}')
        MQTT_ENDPOINT_PORT=443
        HTTP_ENDPOINT_HOST=$(eval kubectl get route -n drogue-iot http-endpoint -o jsonpath='{.status.ingress[0].host}')
        HTTP_ENDPOINT_PORT=443
        ;;
   *)
        echo "Unknown Kubernetes platform: $CLUSTER ... unable to extract endpoints"
        exit 1
        ;;
esac;

HTTP_ENDPOINT_URL=${HTTP_ENDPOINT_HOST}:${HTTP_ENDPOINT_PORT}

BACKEND_URL="$(service_url "console-backend")"
CONSOLE_URL="$(service_url "console")"
SSO_URL="$(ingress_url "keycloak")"

# Provide a TLS certificate for the MQTT endpoint

if [ "$(kubectl -n "$DROGUE_NS" get secret mqtt-endpoint-tls --ignore-not-found)" == "" ] || [ "$(kubectl -n "$DROGUE_NS" get secret http-endpoint-tls --ignore-not-found)" == "" ] ; then
  if [ -z "$TLS_KEY" ] || [ -z "$TLS_CRT" ]; then
    echo "Creating custom certificate..."
    CERT_ALTNAMES="$CERT_ALTNAMES DNS:$MQTT_ENDPOINT_HOST, DNS:$HTTP_ENDPOINT_HOST"
    openssl req -x509 -nodes -days 365 -newkey rsa:2048 -keyout tls_tmp.key -out tls.crt -subj "/CN=foo.bar.com" -addext "subjectAltName = $CERT_ALTNAMES"
    openssl rsa -in tls_tmp.key -out tls.key
    TLS_KEY=tls.key
    TLS_CRT=tls.crt
  else
    echo "Using provided certificate..."
  fi
  # create or update secrets
  kubectl -n "$DROGUE_NS" create secret tls mqtt-endpoint-tls --key "$TLS_KEY" --cert "$TLS_CRT" --dry-run=client -o json | kubectl -n "$DROGUE_NS" apply -f -
  kubectl -n "$DROGUE_NS" create secret tls http-endpoint-tls --key "$TLS_KEY" --cert "$TLS_CRT" --dry-run=client -o json | kubectl -n "$DROGUE_NS" apply -f -
fi

# Update the console endpoints

kubectl -n "$DROGUE_NS" set env deployment/console-backend "HTTP_ENDPOINT_URL=$HTTP_ENDPOINT_URL"
kubectl -n "$DROGUE_NS" set env deployment/console-backend "MQTT_ENDPOINT_HOST=$MQTT_ENDPOINT_HOST"
kubectl -n "$DROGUE_NS" set env deployment/console-backend "MQTT_ENDPOINT_PORT=$MQTT_ENDPOINT_PORT"
kubectl -n "$DROGUE_NS" set env deployment/console-backend "ISSUER_URL=$SSO_URL/auth/realms/drogue"
kubectl -n "$DROGUE_NS" set env deployment/console-backend "REDIRECT_URL=$CONSOLE_URL"

kubectl -n "$DROGUE_NS" set env deployment/console-frontend "BACKEND_URL=$BACKEND_URL"

kubectl -n "$DROGUE_NS" patch keycloakclient/client --type json --patch "[{\"op\": \"replace\",\"path\": \"/spec/client/redirectUris/0\",\"value\": \"$CONSOLE_URL\"}]"

# wait for other Knative services
wait_for_ksvc influxdb-pusher

# wait for the rest of the deployments
kubectl wait deployment -l '!serving.knative.dev/service' --timeout=-1s --for=condition=Available -n "$DROGUE_NS"

# show status

source "$SCRIPTDIR/status.sh"
