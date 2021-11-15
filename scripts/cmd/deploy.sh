#!/usr/bin/env bash

set +e

# defaults

: "${DEPLOY_TWIN:=false}"

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
  -S <key>=<value>   Set a Helm option (as string), can be repeated:
                       -S foo=bar -S bar=baz -S foo.bar=baz
  -k                 Don't install dependencies
  -p <profile>       Enable Helm profile (adds 'deploy/profiles/<profile>.yaml')
  -t <timeout>       Helm installation timeout (default: 15m)
  -T                 Deploy the digital twin feature
  -h                 Show this help

EOF
}

# shellcheck disable=SC2181
[ $? -eq 0 ] || {
    help >&3
    # we don't "fail" but exit here, since we don't want any more output
    exit 1
}

while getopts mhkp:c:n:d:s:S:t:T FLAG; do
  case $FLAG in
    c)
        CLUSTER="$OPTARG"
        ;;
    k)
        INSTALL_DEPS=false
        ;;
    n)
        DROGUE_NS="$OPTARG"
        ;;
    s)
        HELM_ARGS="$HELM_ARGS --set $OPTARG"
        ;;
    S)
        HELM_ARGS="$HELM_ARGS --set-string $OPTARG"
        ;;
    d)
        DOMAIN="$OPTARG"
        ;;
    m)
        MINIMIZE=true
        ;;
    p)
        HELM_PROFILE="$OPTARG"
        ;;
    t)
        HELM_TIMEOUT="$OPTARG"
        ;;
    T)
        DEPLOY_TWIN="true"
        ;;
    h)
        help >&3
        exit 0
        ;;
    \?)
        help >&3
        exit 0
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
: "${INSTALL_DITTO_OPERATOR:=${INSTALL_DEPS}}"
: "${HELM_TIMEOUT:=15m}"

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
if [[ "$INSTALL_DITTO_OPERATOR" == true && "$DEPLOY_TWIN" == true ]]; then
    progress "📦 Deploying pre-requisites (Ditto Operator) ... "
    source "$BASEDIR/cmd/__ditto.sh"
fi

# gather Helm arguments

if [[ -f $BASEDIR/local-values.yaml ]]; then
    progress "💡 Adding local values file ($BASEDIR/local-values.yaml)"
    HELM_ARGS="$HELM_ARGS --values $BASEDIR/local-values.yaml"
fi
if [[ "$HELM_PROFILE" ]]; then
    progress "💡 Adding profile values file ($BASEDIR/../deploy/profiles/${HELM_PROFILE}.yaml)"
    HELM_ARGS="$HELM_ARGS --values $BASEDIR/../deploy/profiles/${HELM_PROFILE}.yaml"
fi
if [[ -f $BASEDIR/../deploy/profiles/${CLUSTER}.yaml ]]; then
    progress "💡 Adding cluster type values file ($BASEDIR/../deploy/profiles/${CLUSTER}.yaml)"
    HELM_ARGS="$HELM_ARGS --values $BASEDIR/../deploy/profiles/${CLUSTER}.yaml"
fi

domain=$(detect_domain)

HELM_ARGS="$HELM_ARGS --timeout=${HELM_TIMEOUT}"
HELM_ARGS="$HELM_ARGS --set global.cluster=$CLUSTER"
HELM_ARGS="$HELM_ARGS --set global.domain=${domain}"
HELM_ARGS="$HELM_ARGS --set coreReleaseName=drogue-iot"
HELM_ARGS="$HELM_ARGS --set drogueCloudExamples.grafana.keycloak.enabled=true --set drogueCloudExamples.grafana.keycloak.client.create=true"
HELM_ARGS="$HELM_ARGS --set drogueCloudTwin.enabled=$DEPLOY_TWIN"

echo "Helm arguments: $HELM_ARGS"

# install Drogue IoT

progress -n "🔨 Deploying Drogue IoT ... "
helm dependency update "$BASEDIR/../deploy/install"
# shellcheck disable=SC2086
helm -n "$DROGUE_NS" upgrade drogue-iot "$BASEDIR/../deploy/install" --install $HELM_ARGS
progress "done!"

# wait for the Keycloak ingress resource

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

# provide TLS certificates for endpoints

if [ "$(kubectl -n "$DROGUE_NS" get secret mqtt-endpoint-tls --ignore-not-found)" == "" ] || [ "$(kubectl -n "$DROGUE_NS" get secret http-endpoint-tls --ignore-not-found)" == "" ] || [ "$(kubectl -n "$DROGUE_NS" get secret coap-endpoint-tls --ignore-not-found)" == "" ]; then
    progress "🔐 Deploying certificates ..."
    progress -n "  📝 Ensure existence of certificates ... "
    if [ -z "$TLS_KEY" ] || [ -z "$TLS_CRT" ]; then
        CERT_ALTNAMES="$CERT_ALTNAMES DNS:$MQTT_ENDPOINT_HOST, DNS:$MQTT_INTEGRATION_HOST, DNS:$HTTP_ENDPOINT_HOST, DNS:$COAP_ENDPOINT_HOST"
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
progress "  * Log in using 'drg':"
progress "      drg login ${API_URL}"
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
