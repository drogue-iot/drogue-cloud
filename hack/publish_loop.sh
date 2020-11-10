#!/usr/bin/env bash

#
# Publish a "temperature" every 5 seconds
#

: "${ENDPOINT:="https://http-endpoint-drogue-iot.apps.wonderful.iot-playground.org"}"
: "${DEVICE_ID:="my:dev1"}"
: "${CHANNEL:="foo"}"
: "${MODEL_ID:="io.drogue.demo:FirstTestDevice:1.0.0"}"

while true; do
  http -v POST "${ENDPOINT}/publish/${DEVICE_ID}/${CHANNEL}" "model_id==$MODEL_ID" temp:="$(printf "%f" "$(echo "s ( $(date +%s ) * 0.02 ) * 10 + 10" | bc -l)")"
  sleep 5
done
