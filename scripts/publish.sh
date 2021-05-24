#!/usr/bin/env bash

set -x

: "${APP:=app_id}"
: "${DEVICE:=device_id}"
: "${PASS:=foobar}"
TEMP=${1:-42}

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
source "$SCRIPTDIR/common.sh"

BACKEND_URL=$(service_url "console-backend")
HTTP_ENDPOINT_URL=$(service_url "http-endpoint" https)

# login
if ! drg token; then
  drg login $BACKEND_URL
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
http --auth ${DEVICE}@${APP}:${PASS} --verify ${SCRIPTDIR}/../build/certs/endpoints/ca-bundle.pem POST ${HTTP_ENDPOINT_URL}/v1/anything temp:=${TEMP}
