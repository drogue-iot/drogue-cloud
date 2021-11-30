#!/usr/bin/env bash

#
# Publish a "temperature" every 5 seconds
#

: "${ENDPOINT:="https://http.sandbox.drogue.cloud"}"
: "${APP:=lora-app}"
: "${DEVICE:=ttn-service}"
: "${PASS:=hey-rodney}"

while true; do
    data=`echo -n "{\"temp\":$((20 + $RANDOM % 5))}" | base64`

    payload="{
        \"app_id\": \"kubecon\",
        \"counter\": 0,
        \"dev_id\": \"kubecon-device\",
        \"downlink_url\": \"https://integrations.thethingsnetwork.org/ttn-eu/api/v2/down/xxx/xxx?key=xxx\",
        \"hardware_serial\": \"000A22A981D142F0\",
        \"metadata\": {
            \"time\": \"2021-04-01T11:30:52.016876672Z\"
        },
        \"payload_raw\": \"$data\",
        \"port\": 1
    }"
   echo "$payload" | http --verbose --auth "${DEVICE}@${APP}:${PASS}" ${CERT:+--verify $CERT} POST ${ENDPOINT}/ttn
   sleep 5
 done
