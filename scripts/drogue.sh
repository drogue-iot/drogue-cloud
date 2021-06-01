#!/usr/bin/env bash

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

  -c    The cluster type (default: $CLUSTER)
        one of: minikube, kind, kubernetes, openshift
  -d    The base directory for the deployment scripts (default: $DEPLOYDIR)

EOF
}

opts=$(getopt "hc:d:" "$*")
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

set -e

command -v 'kubectl' &>/dev/null || die "Missing the command 'kubectl'"
command -v 'curl' &>/dev/null || die "Missing the command 'curl'"
command -v 'jq' &>/dev/null || die "Missing the command 'jq'"
command -v 'sed' &>/dev/null || die "Missing the command 'sed'"
command -v 'docker' &>/dev/null || command -v 'podman' &>/dev/null || die "Missing the command 'docker' or 'podman'"

# Check if we can connect to the cluster

kubectl version &>/dev/null || die "Unable to connect to the cluster: 'kubectl' must be able to connect to your cluster."

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

# Patch some of the deployments to to allow persistent volume access
if [ "$CLUSTER" == "kubernetes" ]; then
    # Wait for the resources to show up
    wait_for_resource deployment/keycloak-postgresql
    wait_for_resource deployment/postgres
    wait_for_resource deployment/grafana

    kubectl -n "$DROGUE_NS" patch deployment keycloak-postgresql -p '{"spec":{"template":{"spec":{"securityContext":{"fsGroup": 2000, "runAsNonRoot": true, "runAsUser": 1000}}}}}'
    kubectl -n "$DROGUE_NS" patch deployment postgres -p '{"spec":{"template":{"spec":{"securityContext":{"fsGroup": 2000, "runAsNonRoot": true, "runAsUser": 1000}}}}}'
    kubectl -n "$DROGUE_NS" patch deployment grafana -p '{"spec":{"template":{"spec":{"securityContext":{"fsGroup": 2000, "runAsNonRoot": true, "runAsUser": 1000}}}}}'
fi

# Remove the wrong host entry for keycloak ingress

case $CLUSTER in
    openshift)
        # we must set the hostname on openshift before calling the "endpoints.sh" script
        kubectl -n "$DROGUE_NS" patch ingress/api --type json --patch '[{"op": "add", "path": "/spec/rules/0/host", "value": "'"$(domain)"'"}]' || true
        wait_for_resource route/keycloak
        ;;
    *)
        wait_for_resource ingress/keycloak
        kubectl -n "$DROGUE_NS" patch ingress/keycloak --type json --patch '[{"op": "remove", "path": "/spec/rules/0/host"}]' || true
        ;;
esac

# source the endpoint information

SILENT=true source "${SCRIPTDIR}/endpoints.sh"

# Provide a TLS certificate for the MQTT endpoint

if [ "$(kubectl -n "$DROGUE_NS" get secret mqtt-endpoint-tls --ignore-not-found)" == "" ] || [ "$(kubectl -n "$DROGUE_NS" get secret http-endpoint-tls --ignore-not-found)" == "" ] ; then
  if [ -z "$TLS_KEY" ] || [ -z "$TLS_CRT" ]; then
    echo "Creating custom certificate..."
    CERT_ALTNAMES="$CERT_ALTNAMES DNS:$MQTT_ENDPOINT_HOST, DNS:$MQTT_INTEGRATION_HOST, DNS:$HTTP_ENDPOINT_HOST"
    "$SCRIPTDIR/gen-certs.sh" "$CERT_ALTNAMES"
    OUT="${SCRIPTDIR}/../build/certs/endpoints"
    MQTT_TLS_KEY=$OUT/mqtt-endpoint.key
    MQTT_TLS_CRT=$OUT/mqtt-endpoint.fullchain.crt
    HTTP_TLS_KEY=$OUT/http-endpoint.key
    HTTP_TLS_CRT=$OUT/http-endpoint.fullchain.crt
  else
    echo "Using provided certificate..."
    MQTT_TLS_KEY=$TLS_KEY
    MQTT_TLS_CRT=$TLS_CRT
    HTTP_TLS_KEY=$TLS_KEY
    HTTP_TLS_CRT=$TLS_CRT
  fi
  # create or update secrets
  kubectl -n "$DROGUE_NS" create secret tls mqtt-endpoint-tls --key "$MQTT_TLS_KEY" --cert "$MQTT_TLS_CRT" --dry-run=client -o json | kubectl -n "$DROGUE_NS" apply -f -
  kubectl -n "$DROGUE_NS" create secret tls http-endpoint-tls --key "$HTTP_TLS_KEY" --cert "$HTTP_TLS_CRT" --dry-run=client -o json | kubectl -n "$DROGUE_NS" apply -f -
fi

# Update the console endpoints

kubectl -n "$DROGUE_NS" set env deployment/console-backend "ENDPOINTS__API_URL=$API_URL"
kubectl -n "$DROGUE_NS" set env deployment/console-backend "ENDPOINTS__HTTP_ENDPOINT_URL=$HTTP_ENDPOINT_URL"
kubectl -n "$DROGUE_NS" set env deployment/console-backend "ENDPOINTS__MQTT_ENDPOINT_HOST=$MQTT_ENDPOINT_HOST" "ENDPOINTS__MQTT_ENDPOINT_PORT=$MQTT_ENDPOINT_PORT"
kubectl -n "$DROGUE_NS" set env deployment/console-backend "ENDPOINTS__MQTT_INTEGRATION_HOST=$MQTT_INTEGRATION_HOST" "ENDPOINTS__MQTT_INTEGRATION_PORT=$MQTT_INTEGRATION_PORT"
kubectl -n "$DROGUE_NS" set env deployment/console-backend "ENDPOINTS__DEVICE_REGISTRY_URL=$API_URL" "ENDPOINTS__COMMAND_ENDPOINT_URL=$COMMAND_ENDPOINT_URL"
kubectl -n "$DROGUE_NS" set env deployment/console-backend "SSO_URL=$SSO_URL" "ENDPOINTS__REDIRECT_URL=$CONSOLE_URL"
kubectl -n "$DROGUE_NS" set env deployment/console-backend "DEMOS=Grafana Dashboard=$DASHBOARD_URL"

kubectl -n "$DROGUE_NS" set env deployment/device-management-service "SSO_URL=$SSO_URL"
kubectl -n "$DROGUE_NS" set env deployment/authentication-service "SSO_URL=$SSO_URL"
kubectl -n "$DROGUE_NS" set env deployment/user-auth-service "SSO_URL=$SSO_URL"
kubectl -n "$DROGUE_NS" set env deployment/command-endpoint "SSO_URL=$SSO_URL"
kubectl -n "$DROGUE_NS" set env deployment/http-endpoint "SSO_URL=$SSO_URL"
if [ "$(kubectl -n drogue-iot get deployment http-insecure-endpoint --ignore-not-found)" != "" ] ; then
  kubectl -n "$DROGUE_NS" set env deployment/http-insecure-endpoint "SSO_URL=$SSO_URL"
fi
kubectl -n "$DROGUE_NS" set env deployment/mqtt-endpoint "SSO_URL=$SSO_URL"

kubectl -n "$DROGUE_NS" set env deployment/mqtt-integration "SSO_URL=$SSO_URL"

kubectl -n "$DROGUE_NS" set env deployment/ttn-operator "SSO_URL=$SSO_URL" "ENDPOINTS__HTTP_ENDPOINT_URL=$HTTP_ENDPOINT_URL"

kubectl -n "$DROGUE_NS" set env deployment/grafana "SSO_URL=$SSO_URL" "GF_SERVER_ROOT_URL=$DASHBOARD_URL"

# we still need to "backend" URL here, since the backend can still do a few things that we don't want in the API
kubectl -n "$DROGUE_NS" set env deployment/console-frontend "BACKEND_URL=$API_URL"

if [ "$CLUSTER" != "openshift" ]; then
    kubectl -n "$DROGUE_NS" annotate ingress/keycloak --overwrite 'nginx.ingress.kubernetes.io/proxy-buffer-size=16k'
fi
kubectl -n "$DROGUE_NS" patch keycloakclient/client --type json --patch "[{\"op\": \"replace\",\"path\": \"/spec/client/redirectUris\",\"value\": [\"${CONSOLE_URL}\", \"${CONSOLE_URL}/*\", \"http://localhost:*\"]}]"
kubectl -n "$DROGUE_NS" patch keycloakclient/client-grafana --type json --patch "[{\"op\": \"replace\",\"path\": \"/spec/client/redirectUris/0\",\"value\": \"$DASHBOARD_URL/login/generic_oauth\"}]"

# set the host names in the ingresses

case $CLUSTER in
    openshift)
        # nothing to do here
        ;;
    *)
        # The host will by applied late, based on the IP of its status section
        kubectl -n "$DROGUE_NS" patch ingress/keycloak --type json --patch '[{"op": "add", "path": "/spec/rules/0/host", "value": "'"${SSO_HOST}"'"}]' || true
        kubectl -n "$DROGUE_NS" patch ingress/api --type json --patch '[{"op": "add", "path": "/spec/rules/0/host", "value": "'"${API_HOST}"'"}]' || true
        ;;
esac;


# wait for other Knative services

wait_for_ksvc influxdb-pusher

# wait for the rest of the deployments

kubectl wait deployment -l '!serving.knative.dev/service' --timeout=-1s --for=condition=Available -n "$DROGUE_NS"

# show status

source "$SCRIPTDIR/status.sh"
