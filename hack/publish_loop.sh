#!/usr/bin/env bash

#
# Publish a "temperature" every 5 seconds
#

DEVICE="dev1"
CHANNEL="foo"

while true; do
  http -v POST "https://http-endpoint-drogue-iot.apps.wonderful.iot-playground.org/publish/${DEVICE}/${CHANNEL}" temp:="$(printf "%f" "$(echo "s ( $(date +%s ) * 0.02 ) * 10 + 10" | bc -l)")"
  sleep 5
done
