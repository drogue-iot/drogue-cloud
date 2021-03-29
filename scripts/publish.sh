#!/usr/bin/env bash

set -x

: "${APP:=app_id}"
: "${DEVICE:=device_id}"
: "${PASS:=foobar}"
TEMP=${1:-42}

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
source "$SCRIPTDIR/common.sh"

MGMT_URL=$(service_url "registry")
HTTP_ENDPOINT_URL=$(service_url "http-endpoint" https)

# app
if ! http --check-status ${MGMT_URL}/api/v1/apps/${APP} >/dev/null 2>&1; then
  http POST ${MGMT_URL}/api/v1/apps metadata:="{\"name\":\"${APP}\"}"
fi

# device
if ! http --check-status ${MGMT_URL}/api/v1/apps/${APP}/devices/${DEVICE} >/dev/null 2>&1; then
  http POST ${MGMT_URL}/api/v1/apps/${APP}/devices \
       metadata:="{\"application\": \"${APP}\", \"name\":\"${DEVICE}\"}" \
       spec:="{\"credentials\": {\"credentials\":[{ \"pass\": \"${PASS}\" }]}}"
fi

# temp
http --auth ${DEVICE}@${APP}:${PASS} --verify build/certs/endpoints/ca-bundle.pem POST ${HTTP_ENDPOINT_URL}/v1/anything temp:=${TEMP}
