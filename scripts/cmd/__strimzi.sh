#!/usr/bin/env bash

set -e

: "${KAFKA_NS:=kafka}"
: "${STRIMZI_VERSION:=0.30.0}"

echo "Installing Strimzi: ${STRIMZI_VERSION}"
progress "📦 Deploying pre-requisites (Strimzi v${STRIMZI_VERSION}) ... "

#
# Strimzi
#
if ! kubectl get ns $KAFKA_NS >/dev/null 2>&1; then
    progress -n "  🆕 Creating namespace ... "
    kubectl create ns $KAFKA_NS
    progress "done!"
fi
if ! kubectl -n $KAFKA_NS get deploy/strimzi-cluster-operator >/dev/null 2>&1; then
    progress -n "  🏗 Deploying the operator ... "
    # use "kubectl create" -> https://github.com/strimzi/strimzi-kafka-operator/issues/4589
    curl -sL "https://github.com/strimzi/strimzi-kafka-operator/releases/download/${STRIMZI_VERSION}/strimzi-cluster-operator-${STRIMZI_VERSION}.yaml" |
        sed "s/myproject/${KAFKA_NS}/" |
        kubectl create -n $KAFKA_NS -f -

    # the following is required to watch all namespaces
    kubectl -n $KAFKA_NS set env deploy/strimzi-cluster-operator STRIMZI_NAMESPACE=\*
    if ! kubectl get clusterrolebinding strimzi-cluster-operator-namespaced >/dev/null 2>&1; then
        kubectl create clusterrolebinding strimzi-cluster-operator-namespaced \
            --clusterrole=strimzi-cluster-operator-namespaced \
            --serviceaccount $KAFKA_NS:strimzi-cluster-operator
        kubectl create clusterrolebinding strimzi-cluster-operator-entity-operator-delegation \
            --clusterrole=strimzi-entity-operator \
            --serviceaccount $KAFKA_NS:strimzi-cluster-operator
        kubectl create clusterrolebinding strimzi-cluster-operator-topic-operator-delegation \
            --clusterrole=strimzi-topic-operator \
            --serviceaccount $KAFKA_NS:strimzi-cluster-operator
    fi
    progress "done!"
fi

if [[ "$MINIMIZE" == true ]]; then
    kubectl -n "$KAFKA_NS" set resources deployment strimzi-cluster-operator --requests=cpu=0
fi

progress -n "  ⏳ Waiting for the operator to become ready ... "
kubectl wait deployment --all --timeout=-1s --for=condition=Available -n "$KAFKA_NS"
progress "done!"
