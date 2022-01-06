#!/usr/bin/env bash

#
# Publish a "temperature" every 5 seconds
#

: "${TYPE:="http"}"
: "${ENDPOINT:="https://http-endpoint-drogue-iot.apps.my.cluster"}"
: "${PORT:=30001}"
: "${SLEEP:=5}"
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
    case ${TYPE} in
    "http")
        http -v --auth "${DEVICE}@${APP}:${PASS}" ${CERT:+--verify $CERT} POST "${ENDPOINT}/v1/${CHANNEL}" "model_id==$MODEL_ID" temp:="$(printf "%f" "$(echo "s ( $(date +%s) * $FACTOR ) * $RANGE + $OFFSET" | bc -l)")" "$@"
        ;;
    "mqtt")
        mqtt pub -v -h ${ENDPOINT} -p ${PORT} -u ${DEVICE}@${APP} -pw ${PASS} -s ${CERT:+--cafile $CERT} -t temp -m "$(printf "%f" "$(echo "s ( $(date +%s) * $FACTOR ) * $RANGE + $OFFSET" | bc -l)")"
        ;;
    esac
    sleep ${SLEEP}
done
