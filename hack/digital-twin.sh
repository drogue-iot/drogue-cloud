#!/usr/bin/env bash

set -e

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"

: "${CLUSTER:="minikube"}"

: "${INSTALL_DEPS:=true}"
: "${INSTALL_DITTO_OPERATOR:=${INSTALL_DEPS}}"

source "$SCRIPTDIR/common.sh"

HELMARGS_DITTO=""
HELMARGS_MONGODB="--set auth.rootPassword=admin123456 --set auth.enabled=false"

case $CLUSTER in
openshift)
  HELMARGS_DITTO="$HELMARGS_DITTO --set openshift.enabled=true"
  HELMARGS_MONGODB="$HELMARGS_MONGODB --set podSecurityContext.enabled=false --set containerSecurityContext.enabled=false"
  ;;
*)
  ;;

esac

if [ "$INSTALL_DITTO_OPERATOR" == true ]; then
echo -n "🧑‍🔧 Deploying Ditto operator... "
helm upgrade --install --wait --timeout 30m --repo https://ctron.github.io/helm-charts ditto-operator ditto-operator --version "^0.1.10" -n "$DROGUE_NS" $HELMARGS_DITTO >/dev/null
echo "OK"
fi

echo -n "📚 Deploying MongoDB instance... "
helm upgrade --install --wait --timeout 30m --repo https://charts.bitnami.com/bitnami mongodb mongodb --version 9 -n "$DROGUE_NS" $HELMARGS_MONGODB >/dev/null
echo "OK"

echo -n "🪞 Deploying digital twin... "
kubectl -n "$DROGUE_NS" apply -k "$SCRIPTDIR/../deploy/digital-twin/" >/dev/null
echo "OK"

# wait for ingress IP to appear

case $CLUSTER in
openshift)
  ;;
*)

echo -n "📥 Waiting for IP in ingress status... "
while [ -z "$(kubectl -n "$DROGUE_NS" get ingress ditto -o jsonpath='{.status.loadBalancer.ingress[0].ip}' 2>/dev/null)" ]; do
    sleep 5
done
echo "OK"

  ;;

esac

# waiting for ditto deployment

echo -n "📥 Waiting for Ditto deployment to become active... "
while [ "$(kubectl -n "$DROGUE_NS" get ditto ditto -o jsonpath='{.status.phase}' 2>/dev/null)" != "Active" ]; do
    sleep 5
done
echo "OK"

# waiting for Ditto API to be available

echo -n "👍 Waiting for the availability of the Ditto API... "
kubectl -n "$DROGUE_NS" wait deployment/ditto-nginx --for=condition=Available --timeout=-1s >/dev/null
echo "OK"

# show status

DIGITAL_TWIN=true source $SCRIPTDIR/status.sh

tput setaf 7 && tput dim || true
echo -----
echo "You can display this information later on by running:"
echo "   env DIGITAL_TWIN=true ./hack/status.sh"
echo
tput sgr0 || true
