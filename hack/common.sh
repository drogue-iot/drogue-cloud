#!/usr/bin/env bash

: "${DROGUE_NS:=drogue-iot}"

function service_url() {
  local name="$1"
  shift

case $CLUSTER in
   kind)
       DOMAIN=$(kubectl get node kind-control-plane -o jsonpath='{.status.addresses[?(@.type == "InternalIP")].address}').nip.io
       PORT=$(kubectl get service -n "$DROGUE_NS" "$name" -o jsonpath='{.spec.ports[0].nodePort}')
       URL=http://$name.$DOMAIN:$PORT
       ;;
   minikube)
        URL=$(eval minikube service -n "$DROGUE_NS" --url "$name")
        ;;
   openshift)
        URL="https://$(kubectl get route -n "$DROGUE_NS" "$name" -o 'jsonpath={ .spec.host }')"
        ;;
   *)
        echo "Unknown Kubernetes platform: $CLUSTER ... unable to extract endpoints"
        exit 1
        ;;
esac;
echo "$URL"
}

function ingress_url() {
  local name="$1"
  shift

case $CLUSTER in
   openshift)
        URL="https://$(kubectl get route -n "$DROGUE_NS" "$name" -o 'jsonpath={ .spec.host }')"
        ;;
   *)
        URL="http://$(kubectl get ingress -n "$DROGUE_NS" "$name"  -o 'jsonpath={ .status.loadBalancer.ingress[0].ip }')"
        ;;
esac;
echo "$URL"
}


function kservice_url() {
  local name="$1"
  shift

URL=$(kubectl get ksvc -n $DROGUE_NS "$name" -o jsonpath='{.status.url}')

case $CLUSTER in
   kind)
        ;;
   minikube)
        ;;
   openshift)
        URL=${URL//http:/https:}
        ;;
   *)
        echo "Unknown Kubernetes platform: $CLUSTER ... unable to extract endpoints"
        exit 1
        ;;
esac;
echo "$URL"
}

function wait_for_resource() {
  local resource="$1"
  shift

  echo "Waiting until $resource exists..."

  set +x
  while ! kubectl get "$resource" -n "$DROGUE_NS" >/dev/null 2>&1; do
    sleep 5
  done
  set -x
}
