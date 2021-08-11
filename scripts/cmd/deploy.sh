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
  -k                 Don't install dependencies
  -p <profile>       Enable Helm profile (adds 'deploy/profiles/<profile>.yaml')
  -h                 Show this help

EOF
}

opts=$(getopt -o "mhkp:c:n:d:s:" -- "$@")
# shellcheck disable=SC2181
[ $? -eq 0 ] || {
    help >&3
    # we don't "fail" but exit here, since we don't want any more output
    exit 1
}
eval set -- "$opts"

while [[ $# -gt 0 ]]; do
    case "$1" in
    -c)
        CLUSTER="$2"
        shift 2
        ;;
    -k)
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
    -m)
        MINIMIZE=true
        shift
        ;;
    -p)
        HELM_PROFILE="$2"
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
        # we don't "fail" but exit here, since we don't want any more output
        exit 1
        ;;
    esac
done

set -e

echo "Minimize: $MINIMIZE"

#
# deploy defaults
#

: "${INSTALL_DEPS:=true}"
: "${INSTALL_STRIMZI:=${INSTALL_DEPS}}"
: "${INSTALL_KNATIVE:=${INSTALL_DEPS}}"
: "${INSTALL_KEYCLOAK_OPERATOR:=${INSTALL_DEPS}}"

case $CLUSTER in
    kind)
        : "${INSTALL_NGINX_INGRESS:=${INSTALL_DEPS}}"
        # test for the ingress controller node flag
        if [[ -z "$(kubectl get node kind-control-plane -o jsonpath="{.metadata.labels['ingress-ready']}")" ]]; then
            die "Kind node 'kind-control-plane' is missing 'ingress-ready' annotation. Please ensure that you properly set up Kind for ingress: https://kind.sigs.k8s.io/docs/user/ingress#create-cluster"
        fi
        ;;
    *)
        ;;
esac

# Check for our standard tools

check_std_tools

# Check if we can connect to the cluster

check_cluster_connection

# Create the namespace first

if ! kubectl get ns "$DROGUE_NS" >/dev/null 2>&1; then
    progress -n "🆕 Creating namespace ($DROGUE_NS) ... "
    kubectl create namespace "$DROGUE_NS"
    kubectl label namespace "$DROGUE_NS" bindings.knative.dev/include=true
    progress "done!"
fi

# install pre-reqs

if [[ "$INSTALL_NGINX_INGRESS" == true ]]; then
    progress "📦 Deploying pre-requisites (NGINX Ingress Controller) ... "
    source "$BASEDIR/cmd/__nginx.sh"
fi
if [[ "$INSTALL_STRIMZI" == true ]]; then
    progress "📦 Deploying pre-requisites (Strimzi) ... "
    source "$BASEDIR/cmd/__strimzi.sh"
fi
if [[ "$INSTALL_KNATIVE" == true ]]; then
    progress "📦 Deploying pre-requisites (Knative) ... "
    source "$BASEDIR/cmd/__knative.sh"
fi
if [[ "$INSTALL_KEYCLOAK_OPERATOR" == true ]]; then
    progress "📦 Deploying pre-requisites (Keycloak) ... "
    source "$BASEDIR/cmd/__sso.sh"
fi

# gather Helm arguments

if [[ -f $BASEDIR/local-values.yaml ]]; then
    progress "💡 Adding local values file ($BASEDIR/local-values.yaml)"
    HELM_ARGS="$HELM_ARGS --values $BASEDIR/local-values.yaml"
fi
if [[ "$HELM_PROFILE" ]]; then
    HELM_ARGS="$HELM_ARGS --values $BASEDIR/../deploy/profiles/${HELM_PROFILE}.yaml"
fi
if [[ -f $BASEDIR/../deploy/profiles/${CLUSTER}.yaml ]]; then
    HELM_ARGS="$HELM_ARGS --values $BASEDIR/../deploy/profiles/${CLUSTER}.yaml"
fi

domain=$(detect_domain)

HELM_ARGS="$HELM_ARGS --set global.cluster=$CLUSTER"
HELM_ARGS="$HELM_ARGS --set global.domain=${domain}"
HELM_ARGS="$HELM_ARGS --set coreReleaseName=drogue-iot"

echo "Helm arguments: $HELM_ARGS"

# install Drogue IoT

progress -n "🔨 Deploying Drogue IoT Core ... "
helm dependency update "$BASEDIR/../deploy/helm/charts/drogue-cloud-core"
# shellcheck disable=SC2086
helm -n "$DROGUE_NS" upgrade drogue-iot "$BASEDIR/../deploy/helm/charts/drogue-cloud-core" --install $HELM_ARGS
progress "done!"

progress -n "🔨 Deploying Drogue IoT Examples ... "
helm dependency update "$BASEDIR/../deploy/helm/charts/drogue-cloud-examples"
# shellcheck disable=SC2086
helm -n "$DROGUE_NS" upgrade drogue-iot-examples "$BASEDIR/../deploy/helm/charts/drogue-cloud-examples" --install $HELM_ARGS
progress "done!"

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

SILENT=true source "${BASEDIR}/cmd/__endpoints.sh"

# Provide TLS certificates for endpoints

if [ "$(kubectl -n "$DROGUE_NS" get secret mqtt-endpoint-tls --ignore-not-found)" == "" ] || [ "$(kubectl -n "$DROGUE_NS" get secret http-endpoint-tls --ignore-not-found)" == "" ] || [ "$(kubectl -n "$DROGUE_NS" get secret coap-endpoint-tls --ignore-not-found)" == "" ]; then
    progress "🔐 Deploying certificates ..."
    progress -n "  📝 Ensure existence of certificates ... "
    if [ -z "$TLS_KEY" ] || [ -z "$TLS_CRT" ]; then
        CERT_ALTNAMES="$CERT_ALTNAMES DNS:$MQTT_ENDPOINT_HOST, DNS:$MQTT_INTEGRATION_HOST, DNS:$HTTP_ENDPOINT_HOST" #, DNS:$COAP_ENDPOINT_HOST"
        echo "  Alternative names: $CERT_ALTNAMES"
        OUT="${BASEDIR}/../build/certs/endpoints"
        echo "  Output: $OUT"

        env TEST_CERTS_IMAGE="${TEST_CERTS_IMAGE}" CONTAINER="$CONTAINER" OUT="$OUT" "$BASEDIR/bin/__gen-certs.sh" "$CERT_ALTNAMES"
        progress "created!"

        COAP_TLS_KEY=$OUT/coap-endpoint.key
        COAP_TLS_CRT=$OUT/coap-endpoint.fullchain.crt
        MQTT_TLS_KEY=$OUT/mqtt-endpoint.key
        MQTT_TLS_CRT=$OUT/mqtt-endpoint.fullchain.crt
        HTTP_TLS_KEY=$OUT/http-endpoint.key
        HTTP_TLS_CRT=$OUT/http-endpoint.fullchain.crt
    else
        progress "provided!"
        COAP_TLS_KEY=$TLS_KEY
        COAP_TLS_CRT=$TLS_CRT
        MQTT_TLS_KEY=$TLS_KEY
        MQTT_TLS_CRT=$TLS_CRT
        HTTP_TLS_KEY=$TLS_KEY
        HTTP_TLS_CRT=$TLS_CRT
    fi
    progress -n "  📝 Deploying certificates ... "
    # create or update secrets
    kubectl -n "$DROGUE_NS" create secret tls coap-endpoint-tls --key "$COAP_TLS_KEY" --cert "$COAP_TLS_CRT" --dry-run=client -o json | kubectl -n "$DROGUE_NS" apply -f -
    kubectl -n "$DROGUE_NS" create secret tls mqtt-endpoint-tls --key "$MQTT_TLS_KEY" --cert "$MQTT_TLS_CRT" --dry-run=client -o json | kubectl -n "$DROGUE_NS" apply -f -
    kubectl -n "$DROGUE_NS" create secret tls http-endpoint-tls --key "$HTTP_TLS_KEY" --cert "$HTTP_TLS_CRT" --dry-run=client -o json | kubectl -n "$DROGUE_NS" apply -f -
    progress "done!"
else
    progress "🔐 Deploying certificates ... unchanged!"
fi

# Update the console endpoints

kubectl -n "$DROGUE_NS" set env deployment/console-backend "DEMOS=Grafana Dashboard=$DASHBOARD_URL"

if [ "$CLUSTER" != "openshift" ]; then
    kubectl -n "$DROGUE_NS" annotate ingress/keycloak --overwrite 'nginx.ingress.kubernetes.io/proxy-buffer-size=16k'
fi

# wait for other Knative services

progress -n "⏳ Waiting for Knative services to become ready ... "
wait_for_ksvc timescaledb-pusher
progress "done!"

# wait for the rest of the deployments

progress -n "⏳ Waiting for deployments to become ready ... "
kubectl wait deployment -l '!serving.knative.dev/service' --timeout=-1s --for=condition=Available -n "$DROGUE_NS"
progress "done!"

# show status

progress "📠 Adding cover sheet to TPS report ... done!"
progress "🥳 Deployment ready!"

progress
progress "To get started, you can:"
progress
progress "  * Navigate to the web console:"
progress "      URL:      ${CONSOLE_URL}"
progress "      User:     admin"
progress "      Password: admin123456"
progress
progress "  * Execute: "
if is_default_cluster; then
progress "      $BASEDIR/drgadm examples"
else
progress "      env CLUSTER=$CLUSTER $BASEDIR/drgadm examples"
fi
progress
