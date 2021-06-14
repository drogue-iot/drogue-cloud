#!/usr/bin/env bash

set +e

# process arguments

help() {
    cat <<EOF
Usage: drgadm deploy
Drogue IoT cloud admin tool - deploy

Options:

  -c <cluster>       The cluster type (default: $CLUSTER)
                       one of: minikube, kind, kubernetes, openshift
  -d <domain>        Set the base DNS domain. Can be auto-detected for Minikube, Kind, and OpenShift.
  -n <namespace>     The namespace to install to (default: $DROGUE_NS)
  -s <key>=<value>   Set a Helm option, can be repeated:
                       -s foo=bar -s bar=baz -s foo.bar=baz
  -p                 Don't install dependencies
  -h                 Show this help

EOF
}

opts=$(getopt "hpc:n:d:s:" "$*")
# shellcheck disable=SC2181
[ $? -eq 0 ] || {
    help >&3
    exit 0
}
eval set --$opts

while [[ $# -gt 0 ]]; do
    case "$1" in
    -c)
        CLUSTER="$2"
        shift 2
        ;;
    -p)
        INSTALL_DEPS=false
        shift
        ;;
    -n)
        DROGUE_NS="$2"
        shift 2
        ;;
    -s)
        HELM_ARGS="$HELM_ARGS --set $2"
        shift 2
        ;;
    -d)
        DOMAIN="$2"
        shift 2
        ;;
    -h)
        help >&3
        exit 0
        ;;
    --)
        shift
        break
        ;;
    *)
        help >&3
        exit 0
        ;;
    esac
done

set -e

#
# deploy defaults
#

: "${INSTALL_DEPS:=true}"
: "${INSTALL_STRIMZI:=${INSTALL_DEPS}}"
: "${INSTALL_KNATIVE:=${INSTALL_DEPS}}"
: "${INSTALL_KEYCLOAK_OPERATOR:=${INSTALL_DEPS}}"

# Check for our standard tools

check_std_tools

# Check if we can connect to the cluster

kubectl version &>/dev/null || die "Unable to connect to the cluster: 'kubectl' must be able to connect to your cluster."

# Create the namespace first

if ! kubectl get ns "$DROGUE_NS" >/dev/null 2>&1; then
    progress -n "🆕 Creating namespace ($DROGUE_NS) ... "
    kubectl create namespace "$DROGUE_NS"
    kubectl label namespace "$DROGUE_NS" bindings.knative.dev/include=true
    progress "done!"
fi

# install pre-reqs

if [[ "$INSTALL_STRIMZI" == true ]]; then
    progress "📦 Deploying pre-requisites (Strimzi) ... "
    source "$SCRIPTDIR/cmd/__strimzi.sh"
fi
if [[ "$INSTALL_KNATIVE" == true ]]; then
    progress "📦 Deploying pre-requisites (Knative) ... "
    source "$SCRIPTDIR/cmd/__knative.sh"
fi
if [[ "$INSTALL_KEYCLOAK_OPERATOR" == true ]]; then
    progress "📦 Deploying pre-requisites (Keycloak) ... "
    source "$SCRIPTDIR/cmd/__sso.sh"
fi

# gather Helm arguments

HELM_ARGS="$HELM_ARGS --set cluster=$CLUSTER"
HELM_ARGS="$HELM_ARGS --set domain=$(detect_domain)"

# install Drogue IoT

progress -n "🔨 Deploying Drogue IoT Core ... "
helm dependency update "$SCRIPTDIR/../deploy/helm/drogue-cloud-core"
# shellcheck disable=SC2086
helm -n "$DROGUE_NS" upgrade drogue-iot "$SCRIPTDIR/../deploy/helm/drogue-cloud-core" --install $HELM_ARGS
progress "done!"

progress -n "🔨 Deploying Drogue IoT Examples ... "
helm dependency update "$SCRIPTDIR/../deploy/helm/drogue-cloud-examples"
# shellcheck disable=SC2086
helm -n "$DROGUE_NS" upgrade drogue-iot-examples "$SCRIPTDIR/../deploy/helm/drogue-cloud-examples" --install $HELM_ARGS
progress "done!"

# Patch some of the deployments to to allow persistent volume access
if [ "$CLUSTER" == "kubernetes" ]; then
    # FIXME: this should be customized by the Helm chart now

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
        progress -n "👀 Waiting for keycloak Route resource ..."
        wait_for_resource route/keycloak
        progress "done!"
        ;;
    *)
        progress -n "👀 Waiting for keycloak Ingress resource ... "
        wait_for_resource ingress/keycloak
        progress "done!"
        ;;
esac

# source the endpoint information

SILENT=true source "${SCRIPTDIR}/cmd/__endpoints.sh"

# Provide a TLS certificate for the MQTT endpoint

if [ "$(kubectl -n "$DROGUE_NS" get secret mqtt-endpoint-tls --ignore-not-found)" == "" ] || [ "$(kubectl -n "$DROGUE_NS" get secret http-endpoint-tls --ignore-not-found)" == "" ]; then
    if [ -z "$TLS_KEY" ] || [ -z "$TLS_CRT" ]; then
        CERT_ALTNAMES="$CERT_ALTNAMES DNS:$MQTT_ENDPOINT_HOST, DNS:$MQTT_INTEGRATION_HOST, DNS:$HTTP_ENDPOINT_HOST"
        echo "  Alternative names: $CERT_ALTNAMES"
        OUT="${SCRIPTDIR}/../build/certs/endpoints"
        echo "  Output: $OUT"

        progress -n "📝 Creating custom certificate ... "
        env CONTAINER="$CONTAINER" OUT="$OUT" "$SCRIPTDIR/bin/__gen-certs.sh" "$CERT_ALTNAMES"
        progress "done!"

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

kubectl -n "$DROGUE_NS" set env deployment/console-backend "ENDPOINTS__REDIRECT_URL=$CONSOLE_URL"

kubectl -n "$DROGUE_NS" set env deployment/console-backend "ENDPOINTS__HTTP_ENDPOINT_URL=$HTTP_ENDPOINT_URL"
kubectl -n "$DROGUE_NS" set env deployment/console-backend "ENDPOINTS__MQTT_ENDPOINT_HOST=$MQTT_ENDPOINT_HOST" "ENDPOINTS__MQTT_ENDPOINT_PORT=$MQTT_ENDPOINT_PORT"
kubectl -n "$DROGUE_NS" set env deployment/console-backend "ENDPOINTS__MQTT_INTEGRATION_HOST=$MQTT_INTEGRATION_HOST" "ENDPOINTS__MQTT_INTEGRATION_PORT=$MQTT_INTEGRATION_PORT"
kubectl -n "$DROGUE_NS" set env deployment/console-backend "ENDPOINTS__DEVICE_REGISTRY_URL=$API_URL" "ENDPOINTS__COMMAND_ENDPOINT_URL=$COMMAND_ENDPOINT_URL"

kubectl -n "$DROGUE_NS" set env deployment/console-backend "DEMOS=Grafana Dashboard=$DASHBOARD_URL"

kubectl -n "$DROGUE_NS" set env deployment/ttn-operator "ENDPOINTS__HTTP_ENDPOINT_URL=$HTTP_ENDPOINT_URL"

if [ "$CLUSTER" != "openshift" ]; then
    kubectl -n "$DROGUE_NS" annotate ingress/keycloak --overwrite 'nginx.ingress.kubernetes.io/proxy-buffer-size=16k'
fi
kubectl -n "$DROGUE_NS" patch keycloakclient/client --type json --patch "[{\"op\": \"replace\",\"path\": \"/spec/client/redirectUris\",\"value\": [\"${CONSOLE_URL}\", \"${CONSOLE_URL}/*\", \"http://localhost:*\"]}]"
kubectl -n "$DROGUE_NS" patch keycloakclient/client-grafana --type json --patch "[{\"op\": \"replace\",\"path\": \"/spec/client/redirectUris/0\",\"value\": \"$DASHBOARD_URL/login/generic_oauth\"}]"

# wait for other Knative services

progress -n "⏳ Waiting for Knative services to become ready ... "
wait_for_ksvc influxdb-pusher
progress "done!"

# wait for the rest of the deployments

progress -n "⏳ Waiting for deployments to become ready ... "
kubectl wait deployment -l '!serving.knative.dev/service' --timeout=-1s --for=condition=Available -n "$DROGUE_NS"
progress "done!"

# show status

progress "📒 Adding cover sheet to TPS report ... done!"
progress "🥳 Deployment ready!"

progress
progress "To get started you can:"
progress
progress "  * Navigate to the web console:"
progress "      URL:      ${CONSOLE_URL}"
progress "      User:     admin"
progress "      Password: admin123456"
progress
progress "  * Execute: "
if is_default_cluster; then
progress "      $SCRIPTDIR/drgadm examples"
else
progress "      env CLUSTER=$CLUSTER $SCRIPTDIR/drgadm status"
fi
progress
