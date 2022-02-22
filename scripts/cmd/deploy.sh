#!/usr/bin/env bash

set +e

# defaults

: "${DEPLOY_TWIN:=false}"
: "${DEPLOY_EXAMPLES:=true}"
: "${DEPLOY_METRICS:=false}"

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
  -e                 Don't install examples
  -f <vales.yaml>    Add a Helm values files
  -p <profile>       Enable Helm profile (adds 'deploy/profiles/<profile>.yaml')
  -t <timeout>       Helm installation timeout (default: 15m)
  -T                 Deploy the digital twin feature
  -M                 Deploy metrics
  -h                 Show this help

EOF
}

# shellcheck disable=SC2181
[ $? -eq 0 ] || {
    help >&3
    # we don't "fail" but exit here, since we don't want any more output
    exit 1
}

while getopts mhkeMp:c:n:d:s:S:t:Tf: FLAG; do
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
    f)
        HELM_ARGS="$HELM_ARGS --values $OPTARG"
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
    M)
        DEPLOY_METRICS="true"
        ;;
    e)
        DEPLOY_EXAMPLES="false"
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
: "${INSTALL_KEYCLOAK_OPERATOR:=${INSTALL_DEPS}}"
: "${INSTALL_DITTO_OPERATOR:=${INSTALL_DEPS}}"
: "${HELM_TIMEOUT:=15m}"

if [[ "$DEPLOY_TWIN" == true ]] || [[ "$DEPLOY_EXAMPLES" == true ]]; then
    # default to global default
    : "${INSTALL_KNATIVE:=${INSTALL_DEPS}}"
else
    # default to false
    : "${INSTALL_KNATIVE:=false}"
fi

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
    source "$BASEDIR/cmd/__nginx.sh"
fi
if [[ "$INSTALL_STRIMZI" == true ]]; then
    source "$BASEDIR/cmd/__strimzi.sh"
fi
if [[ "$INSTALL_KNATIVE" == true ]]; then
    source "$BASEDIR/cmd/__knative.sh"
fi
if [[ "$INSTALL_KEYCLOAK_OPERATOR" == true ]]; then
    source "$BASEDIR/cmd/__sso.sh"
fi
if [[ "$INSTALL_DITTO_OPERATOR" == true && "$DEPLOY_TWIN" == true ]]; then
    source "$BASEDIR/cmd/__ditto.sh"
fi

# add Helm value files
#
# As these are applies in the order of the command line, the more default ones must come first. As we might already
# have value files from the arguments, we prepend our default value files to the arguments, more specific ones first,
# so we end up with an argument list of more specific ones last. Values provide with the --set argument will always
# override value files properties, so their relation to the values files doesn't matter.

if [[ -f $BASEDIR/local-values.yaml ]]; then
    progress "💡 Adding local values file ($BASEDIR/local-values.yaml)"
    HELM_ARGS="--values $BASEDIR/local-values.yaml $HELM_ARGS"
fi
if [[ "$HELM_PROFILE" ]]; then
    progress "💡 Adding profile values file ($BASEDIR/../deploy/profiles/${HELM_PROFILE}.yaml)"
    HELM_ARGS="--values $BASEDIR/../deploy/profiles/${HELM_PROFILE}.yaml $HELM_ARGS"
fi
if [[ -f $BASEDIR/../deploy/profiles/${CLUSTER}.yaml ]]; then
    progress "💡 Adding cluster type values file ($BASEDIR/../deploy/profiles/${CLUSTER}.yaml)"
    HELM_ARGS="--values $BASEDIR/../deploy/profiles/${CLUSTER}.yaml $HELM_ARGS"
fi

# gather Helm arguments

domain=$(detect_domain)

HELM_ARGS="$HELM_ARGS --timeout=${HELM_TIMEOUT}"
HELM_ARGS="$HELM_ARGS --set global.cluster=$CLUSTER"
HELM_ARGS="$HELM_ARGS --set global.domain=${domain}"
HELM_ARGS="$HELM_ARGS --set coreReleaseName=drogue-iot"
HELM_ARGS="$HELM_ARGS --set drogueCloudTwin.enabled=$DEPLOY_TWIN"
HELM_ARGS="$HELM_ARGS --set drogueCloudExamples.enabled=$DEPLOY_EXAMPLES"
HELM_ARGS="$HELM_ARGS --set drogueCloudMetrics.enabled=$DEPLOY_METRICS"
HELM_ARGS="$HELM_ARGS --set drogueCloudMetrics.grafana.ingress.hosts={metrics${domain}}"

echo "Helm arguments: $HELM_ARGS"

# install Drogue IoT

progress "🔨 Deploying Drogue IoT... "
progress "  ☕ This will take a while!"
progress "  🔬 Track its progress using \`watch kubectl -n $DROGUE_NS get pods\`! "
progress -n "  🚀 Performing deployment... "
helm dependency update "$BASEDIR/../deploy/install"
helm dependency update "$BASEDIR/../deploy/helm/charts/drogue-cloud-metrics"
set -x
# shellcheck disable=SC2086
helm -n "$DROGUE_NS" upgrade drogue-iot "$BASEDIR/../deploy/install" --install $HELM_ARGS
set +x
progress "done!"

# source the endpoint information

SILENT=true source "${BASEDIR}/cmd/__endpoints.sh"

# Update the console endpoints

kubectl -n "$DROGUE_NS" set env deployment/console-backend "DEMOS=Grafana Dashboard=$DASHBOARD_URL"

if [ "$CLUSTER" != "openshift" ]; then
    kubectl -n "$DROGUE_NS" annotate ingress/sso --overwrite 'nginx.ingress.kubernetes.io/proxy-buffer-size=16k'
fi

# wait for other Knative services

if [[ "$INSTALL_KNATIVE" == true ]]; then
    progress -n "⏳ Waiting for Knative services to become ready ... "
    wait_for_ksvc timescaledb-pusher
    progress "done!"
fi

# wait for the rest of the deployments

progress -n "⏳ Waiting for deployments to become ready ... "
kubectl wait deployment -l '!serving.knative.dev/service' --timeout=-1s --for=condition=Available -n "$DROGUE_NS"
progress "done!"

# download certificates

mkdir -p build/certs/endpoints/
kubectl -n drogue-iot get configmap trust-anchor -o jsonpath="{.data.root-cert\\.pem}" > build/certs/endpoints/root-cert.pem
kubectl -n drogue-iot get secret http-endpoint-tls -o jsonpath="{.data.tls\\.crt}" | base64 -d > build/certs/endpoints/http-endpoint.crt
kubectl -n drogue-iot get secret mqtt-endpoint-tls -o jsonpath="{.data.tls\\.crt}" | base64 -d > build/certs/endpoints/mqtt-endpoint.crt
kubectl -n drogue-iot get secret coap-endpoint-tls -o jsonpath="{.data.tls\\.crt}" | base64 -d > build/certs/endpoints/coap-endpoint.crt

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
progress "  * Log in using 'drg':"
progress "      * Get it from: https://github.com/drogue-iot/drg/releases/latest"
progress "      * Run:         drg login ${API_URL}"
progress
progress "  * Execute (to see more examples): "

ENVS=""
if [[ "$DEPLOY_TWIN" == true ]]; then
    ENVS+="DIGITAL_TWIN=true "
fi

if [[ "$DEPLOY_EXAMPLES" == false ]]; then
    ENVS+="EXAMPLES=false "
fi

if [[ "$DEPLOY_METRICS" == true ]]; then
    ENVS+="METRICS=true "
fi

if ! is_default_cluster; then
    ENVS+="CLUSTER=$CLUSTER"
fi

if [[ -n "$ENVS" ]]; then
    ENVS="env $ENVS "
fi

progress "      ${ENVS} $BASEDIR/drgadm examples"
progress
