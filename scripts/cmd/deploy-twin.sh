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

opts=$(getopt "hpc:n:d:s:" -- "$@")
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
        # we don't "fail" but exit here, since we don't want any more output
        exit 1
        ;;
    esac
done

set -e

#
# deploy defaults
#

: "${INSTALL_DEPS:=true}"
: "${INSTALL_DITTO_OPERATOR:=${INSTALL_DEPS}}"
: "${INSTALL_MONGODB:=${INSTALL_DEPS}}"

# Check for our standard tools

check_std_tools

# Check if we can connect to the cluster

check_cluster_connection

# Helm args

HELMARGS_DITTO=""
HELMARGS_MONGODB="--set auth.rootPassword=admin123456 --set auth.enabled=false"

case $CLUSTER in
openshift)
    HELMARGS_DITTO="$HELMARGS_DITTO --set openshift.enabled=true"
    HELMARGS_MONGODB="$HELMARGS_MONGODB --set podSecurityContext.enabled=false --set containerSecurityContext.enabled=false"
    ;;
*) ;;
esac

# install pre-reqs

if [[ "$INSTALL_DITTO_OPERATOR" == true ]]; then
    progress -n "📦 Deploying pre-requisites (Ditto operator) ... "
    # shellcheck disable=SC2086
    helm upgrade --install --wait --timeout 30m --repo https://ctron.github.io/helm-charts ditto-operator ditto-operator --version "^0.1.10" -n "$DROGUE_NS" $HELMARGS_DITTO >/dev/null
    progress "done!"
fi

if [[ "$INSTALL_MONGODB" == true ]]; then
    progress -n "📦 Deploying pre-requisites (MongoDB) ... "
    # shellcheck disable=SC2086
    helm upgrade --install --wait --timeout 30m --repo https://charts.bitnami.com/bitnami mongodb mongodb --version 9 -n "$DROGUE_NS" $HELMARGS_MONGODB >/dev/null
    progress "done!"
fi

# Install twin feature

HELM_ARGS="$HELM_ARGS --set cluster=$CLUSTER"
HELM_ARGS="$HELM_ARGS --set domain=$(detect_domain)"

progress -n "🔨 Deploying Drogue IoT Twin ... "
helm dependency update "$SCRIPTDIR/../deploy/helm/drogue-cloud-twin"
set -x
# shellcheck disable=SC2086
helm -n "$DROGUE_NS" upgrade drogue-iot "$SCRIPTDIR/../deploy/helm/drogue-cloud-twin" --install $HELM_ARGS
set +x
progress "done!"

# waiting for ditto deployment

progress -n "📥 Waiting for Ditto deployment to become active ... "
while [ "$(kubectl -n "$DROGUE_NS" get ditto ditto -o jsonpath='{.status.phase}' 2>/dev/null)" != "Active" ]; do
    sleep 5
done
progress "done!"

# waiting for Ditto API to be available

progress -n "👍 Waiting for the availability of the Ditto API ... "
kubectl -n "$DROGUE_NS" wait deployment/ditto-nginx --for=condition=Available --timeout=-1s >/dev/null
progress "done!"
