#!/usr/bin/env bash

set -x

: "${APP:=example-app}"
: "${DEVICE:=device1}"
: "${PASS:=keycloak =}"
TEMP=${1:-42}

BASEDIR="$(cd "$(dirname "${BASH_SOURCE[0]}")"/.. >/dev/null 2>&1 && pwd)"
source "$BASEDIR/lib/mod.sh"

BACKEND_URL="$(get_env deploy/console-backend endpoint ENDPOINTS__API_URL)"
HTTP_ENDPOINT_URL="$(get_env deploy/console-backend endpoint ENDPOINTS__HTTP_ENDPOINT_URL)"

# login
if ! drg whoami; then
    drg login "$BACKEND_URL"
fi

# app
if ! drg get app ${APP}; then
    drg create app ${APP}
    drg get app ${APP}
fi

# device
if ! drg get device --app ${APP} ${DEVICE}; then
    drg create device --app ${APP} ${DEVICE} --data \
        "{\"credentials\": {\"credentials\":[{ \"pass\": \"${PASS}\" }]}}"
    drg get device --app ${APP} ${DEVICE}
fi

# temp
http --auth ${DEVICE}@${APP}:${PASS} --verify "${BASEDIR}/../build/certs/endpoints/ca-bundle.pem" POST ${HTTP_ENDPOINT_URL}/v1/anything temp:=${TEMP}
