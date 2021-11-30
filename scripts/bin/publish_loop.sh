#!/usr/bin/env bash

#
# Publish a "temperature" every 5 seconds
#

: "${ENDPOINT:="https://http-endpoint-drogue-iot.apps.my.cluster"}"
: "${CHANNEL:="foo"}"
: "${MODEL_ID:="io.drogue.demo:FirstTestDevice:1.0.0"}"
: "${APP:=example-app}"
: "${DEVICE:=device1}"
: "${PASS:=hey-rodney}"
: "${OFFSET:=10}"
: "${RANGE:=10}"
: "${FACTOR:=0.02}"

while true; do
    # shellcheck disable=SC2086
    http -v --auth "${DEVICE}@${APP}:${PASS}" ${CERT:+--verify $CERT} POST "${ENDPOINT}/v1/${CHANNEL}" "model_id==$MODEL_ID" temp:="$(printf "%f" "$(echo "s ( $(date +%s) * $FACTOR ) * $RANGE + $OFFSET" | bc -l)")" "$@"
    sleep 5
done
