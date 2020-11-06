#!/usr/bin/env bash

#
# Publish a "temperature" every 5 seconds
#

DEVICE="my:dev1"
CHANNEL="foo"
MODEL_ID="vorto.private.ctron:DeviceOne:1.0.0"

while true; do
  http -v POST "https://http-endpoint-drogue-iot.apps.wonderful.iot-playground.org/publish/${DEVICE}/${CHANNEL}" "model_id==$MODEL_ID" temp:="$(printf "%f" "$(echo "s ( $(date +%s ) * 0.02 ) * 10 + 10" | bc -l)")"
  sleep 5
done
