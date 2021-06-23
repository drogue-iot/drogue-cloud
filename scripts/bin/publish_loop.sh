#!/usr/bin/env bash

#
# Publish a "temperature" every 5 seconds
#

: "${ENDPOINT:="https://http-endpoint-drogue-iot.apps.my.cluster"}"
: "${CHANNEL:="foo"}"
: "${MODEL_ID:="io.drogue.demo:FirstTestDevice:1.0.0"}"
: "${APP:=app_id}"
: "${DEVICE:=device_id}"
: "${PASS:=foobar}"

while true; do
    http -v --auth "${DEVICE}@${APP}:${PASS}" ${CERT:+--verify $CERT} POST "${ENDPOINT}/v1/${CHANNEL}" "model_id==$MODEL_ID" temp:="$(printf "%f" "$(echo "s ( $(date +%s) * 0.02 ) * 10 + 10" | bc -l)")"
    sleep 5
done
