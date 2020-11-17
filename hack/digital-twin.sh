#!/usr/bin/env bash

set -e

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"

: "${PLATFORM:="kubernetes"}"

source "$SCRIPTDIR/common.sh"

HELMARGS_DITTO=""
HELMARGS_MONGODB="--set auth.rootPassword=admin123456 --set auth.enabled=false"

case $PLATFORM in
openshift)
  HELMARGS_DITTO="--set openshift.enabled=true"
  HELMARGS_MONGODB="--set podSecurityContext.enabled=false --set containerSecurityContext.enabled=false"
  ;;
*)
  ;;

esac

helm upgrade --install --wait --timeout 30m --repo https://ctron.github.io/helm-charts ditto-operator ditto-operator --version "^0.1.9" -n "$DROGUE_NS" $HELMARGS_DITTO
helm upgrade --install --wait --timeout 30m --repo https://charts.bitnami.com/bitnami mongodb mongodb --version 9 -n "$DROGUE_NS" $HELMARGS_MONGODB

kubectl -n "$DROGUE_NS" apply -k "$SCRIPTDIR/../deploy/digital-twin/"

# waiting for ditto operator

echo -n "🧑‍🔧 Waiting for the Ditto operator to start... "
kubectl -n "$DROGUE_NS" wait deployment/ditto-operator --for=condition=Available --timeout=-1s &>/dev/null
echo "OK"

# wait for ingress IP to appear

echo -n "📥 Waiting for IP in ingress status... "
while [ -z "$(kubectl -n "$DROGUE_NS" get ingress ditto -o jsonpath='{.status.loadBalancer.ingress[0].ip}' 2>/dev/null)" ]; do
    sleep 5
done
echo "OK"

DIGITAL_TWIN=true source $SCRIPTDIR/status.sh
