#!/usr/bin/env bash

#
# Install the keycloak operator
#

set -e

: "${KEYCLOAK_OPERATOR_VERSION:=17.0.0}"
: "${KEYCLOAK_CRDS:=true}"

if [[ "$KEYCLOAK_CRDS" == true ]]; then

    progress -n "  🗂 Deploying CRDs ... "
    kubectl apply \
        -f "https://raw.githubusercontent.com/keycloak/keycloak-operator/${KEYCLOAK_OPERATOR_VERSION}/deploy/crds/keycloak.org_keycloakbackups_crd.yaml" \
        -f "https://raw.githubusercontent.com/keycloak/keycloak-operator/${KEYCLOAK_OPERATOR_VERSION}/deploy/crds/keycloak.org_keycloakclients_crd.yaml" \
        -f "https://raw.githubusercontent.com/keycloak/keycloak-operator/${KEYCLOAK_OPERATOR_VERSION}/deploy/crds/keycloak.org_keycloakrealms_crd.yaml" \
        -f "https://raw.githubusercontent.com/keycloak/keycloak-operator/${KEYCLOAK_OPERATOR_VERSION}/deploy/crds/keycloak.org_keycloaks_crd.yaml" \
        -f "https://raw.githubusercontent.com/keycloak/keycloak-operator/${KEYCLOAK_OPERATOR_VERSION}/deploy/crds/keycloak.org_keycloakusers_crd.yaml"
    progress "done!"

fi

progress -n "  🏗 Deploying the operator ... "
kubectl -n "$DROGUE_NS" apply \
    -f "https://raw.githubusercontent.com/keycloak/keycloak-operator/${KEYCLOAK_OPERATOR_VERSION}/deploy/service_account.yaml" \
    -f "https://raw.githubusercontent.com/keycloak/keycloak-operator/${KEYCLOAK_OPERATOR_VERSION}/deploy/role.yaml" \
    -f "https://raw.githubusercontent.com/keycloak/keycloak-operator/${KEYCLOAK_OPERATOR_VERSION}/deploy/role_binding.yaml" \
    -f "https://raw.githubusercontent.com/keycloak/keycloak-operator/${KEYCLOAK_OPERATOR_VERSION}/deploy/operator.yaml"
progress "done!"

progress -n "  ⏳ Waiting for the operator to become ready ... "
kubectl -n "$DROGUE_NS" wait deployment keycloak-operator --for=condition=Available --timeout=-1s
progress "done!"
