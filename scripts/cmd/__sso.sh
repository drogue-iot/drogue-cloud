#!/usr/bin/env bash

#
# Install the keycloak operator
#

set -e

: "${KEYCLOAK_OPERATOR_VERSION:=18.0.1}"
: "${KEYCLOAK_CRDS:=true}"

progress "📦 Deploying pre-requisites (Keycloak v${KEYCLOAK_OPERATOR_VERSION}) ... "

if [[ "$KEYCLOAK_CRDS" == true ]]; then

    progress -n "  🗂 Deploying CRDs ... "

    kubectl apply \
        -f "https://raw.githubusercontent.com/keycloak/keycloak-k8s-resources/${KEYCLOAK_OPERATOR_VERSION}/kubernetes/keycloaks.k8s.keycloak.org-v1.yml" \
        -f "https://raw.githubusercontent.com/keycloak/keycloak-k8s-resources/${KEYCLOAK_OPERATOR_VERSION}/kubernetes/keycloakrealmimports.k8s.keycloak.org-v1.yml"
    progress "done!"

fi

progress -n "  🏗 Deploying the operator ... "
kubectl -n "$DROGUE_NS" apply \
    -f https://raw.githubusercontent.com/keycloak/keycloak-k8s-resources/${KEYCLOAK_OPERATOR_VERSION}/kubernetes/kubernetes.yml
progress "done!"

progress -n "  ⏳ Waiting for the operator to become ready ... "
kubectl -n "$DROGUE_NS" wait deployment keycloak-operator --for=condition=Available --timeout=-1s
progress "done!"
