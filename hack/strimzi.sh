#!/usr/bin/env bash

set -ex

: "${KAFKA_NS:=kafka}"
: "${STRIMZI_VERSION:=0.20.0}"
: "${CLUSTER:=minikube}"

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
DEPLOYDIR="$SCRIPTDIR/../deploy"

#
# Strimzi
#

# create namespace
if ! kubectl get ns $KAFKA_NS >/dev/null 2>&1; then kubectl create ns $KAFKA_NS; fi

helm upgrade --install --wait --timeout 30m \
  strimzi \
  https://github.com/strimzi/strimzi-kafka-operator/releases/download/0.20.0/strimzi-kafka-operator-helm-3-chart-0.20.0.tgz \
  --set watchAnyNamespace=true \
  -n "$KAFKA_NS" \
  >/dev/null
