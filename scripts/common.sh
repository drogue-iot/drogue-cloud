#!/usr/bin/env bash

#
# This is the central location defining which cluster type we use.
#
# During the creation of the installer, the default of this will be overridden.
#
: "${CLUSTER:=minikube}"

: "${DROGUE_NS:=drogue-iot}"
: "${CONTAINER:=docker}"
: "${TEST_CERTS_IMAGE:=ghcr.io/drogue-iot/test-cert-generator:latest}"

die() { echo "$*" 1>&2 ; exit 1; }

function service_url() {
  local name="$1"
  shift
  local scheme="$1"

case $CLUSTER in
    kubernetes)
        DOMAIN=$(kubectl get nodes -o jsonpath='{.items[0].status.addresses[?(@.type == "ExternalIP")].address}').nip.io
        PORT=$(kubectl get service -n "$DROGUE_NS" "$name" -o jsonpath='{.spec.ports[0].nodePort}')
        URL=${scheme:-http}://$name.$DOMAIN:$PORT
        ;;
    kind)
        DOMAIN=$(kubectl get node kind-control-plane -o jsonpath='{.status.addresses[?(@.type == "InternalIP")].address}').nip.io
        PORT=$(kubectl get service -n "$DROGUE_NS" "$name" -o jsonpath='{.spec.ports[0].nodePort}')
        URL=${scheme:-http}://$name.$DOMAIN:$PORT
        ;;
    minikube)
         test -n "$scheme" && scheme="--$scheme"
         URL=$(eval minikube service -n "$DROGUE_NS" $scheme --url "$name")
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
        URL="$(kubectl get route -n "$DROGUE_NS" "$name" -o 'jsonpath={ .spec.host }')"
        if [ -n "$URL" ]; then
          URL="https://$URL"
        fi
        ;;
   kubernetes)
        # Workaround to use the node-port service
        if [ "$name" == "keycloak" ]; then
            name="$name-endpoint"
        fi
        DOMAIN=$(kubectl get nodes -o jsonpath='{.items[0].status.addresses[?(@.type == "ExternalIP")].address}').nip.io
        PORT=$(kubectl get service -n "$DROGUE_NS" "$name" -o jsonpath='{.spec.ports[0].nodePort}')
        if [ -n "$DOMAIN" ]; then
          URL="http://$name.$DOMAIN:$PORT"
        fi
        ;;

   kind)
        # Workaround to use the node-port service
        if [ "$name" == "keycloak" ]; then
            name="$name-endpoint"
        fi
        DOMAIN=$(kubectl get node kind-control-plane -o jsonpath='{.status.addresses[?(@.type == "InternalIP")].address}').nip.io
        PORT=$(kubectl get service -n "$DROGUE_NS" "$name" -o jsonpath='{.spec.ports[0].nodePort}')
        if [ -n "$DOMAIN" ]; then
          URL="http://$name.$DOMAIN:$PORT"
        fi
        ;;
   *)
        URL=$(kubectl get ingress -n "$DROGUE_NS" "$name"  -o 'jsonpath={ .status.loadBalancer.ingress[0].ip }')
        if [ -n "$URL" ]; then
          URL="http://$URL"
        fi
        ;;
esac;
echo "$URL"
}


function kservice_url() {
  local name="$1"
  shift

URL=$(kubectl get ksvc -n $DROGUE_NS "$name" -o jsonpath='{.status.url}')

case $CLUSTER in
   kubernetes)
       HTTP_ENDPOINT_PORT=$(kubectl get service -n kourier-system kourier -o jsonpath='{.spec.ports[?(@.name == "http2")].nodePort}')
       URL=${URL}:${HTTP_ENDPOINT_PORT}
        ;;
   kind)
       HTTP_ENDPOINT_PORT=$(kubectl get service -n kourier-system kourier -o jsonpath='{.spec.ports[?(@.name == "http2")].nodePort}')
       URL=${URL}:${HTTP_ENDPOINT_PORT}
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

  while ! kubectl get "$resource" -n "$DROGUE_NS" >/dev/null 2>&1; do
    sleep 5
  done
}

# we nudge (delete the deploys) because of: https://github.com/knative/serving/issues/10344
# TODO: when 10344 is fixed, replace the while loop with the 'kubectl wait'
function wait_for_ksvc() {
  local resource="$1"
  if [ -z "$2" ] ; then
    local timeout=$(($(date +%s) + 600))
  else
    local timeout=$(($(date +%s) + $2))
  fi
  shift

  while (( ${timeout} > $(date +%s) )) ; do
    if ! kubectl -n "$DROGUE_NS" wait --timeout=60s --for=condition=Ready "ksvc/${resource}"; then
      kubectl -n "$DROGUE_NS" delete deploy -l "serving.knative.dev/service=${resource}"
    else
      break
    fi
  done

  if [ ${timeout} \< "$(date +%s)" ] ; then
    echo "Error: timed out while waiting for ${resource} to become ready."
    exit 1
  fi
}
