#!/usr/bin/env bash

#
# Install the keycloak operator
#

set -ex

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
source "$SCRIPTDIR/common.sh"

: "${KEYCLOAK_OPERATOR_VERSION:=12.0.1}"
: "${CLUSTER:=minikube}"
: "${KEYCLOAK_CRDS:=true}"

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"

if [[ "$KEYCLOAK_CRDS" == true ]]; then

kubectl apply \
  -f "https://raw.githubusercontent.com/keycloak/keycloak-operator/${KEYCLOAK_OPERATOR_VERSION}/deploy/crds/keycloak.org_keycloakbackups_crd.yaml" \
  -f "https://raw.githubusercontent.com/keycloak/keycloak-operator/${KEYCLOAK_OPERATOR_VERSION}/deploy/crds/keycloak.org_keycloakclients_crd.yaml" \
  -f "https://raw.githubusercontent.com/keycloak/keycloak-operator/${KEYCLOAK_OPERATOR_VERSION}/deploy/crds/keycloak.org_keycloakrealms_crd.yaml" \
  -f "https://raw.githubusercontent.com/keycloak/keycloak-operator/${KEYCLOAK_OPERATOR_VERSION}/deploy/crds/keycloak.org_keycloaks_crd.yaml" \
  -f "https://raw.githubusercontent.com/keycloak/keycloak-operator/${KEYCLOAK_OPERATOR_VERSION}/deploy/crds/keycloak.org_keycloakusers_crd.yaml" \

fi

kubectl -n "$DROGUE_NS" apply \
  -f "https://raw.githubusercontent.com/keycloak/keycloak-operator/${KEYCLOAK_OPERATOR_VERSION}/deploy/service_account.yaml" \
  -f "https://raw.githubusercontent.com/keycloak/keycloak-operator/${KEYCLOAK_OPERATOR_VERSION}/deploy/role.yaml" \
  -f "https://raw.githubusercontent.com/keycloak/keycloak-operator/${KEYCLOAK_OPERATOR_VERSION}/deploy/role_binding.yaml" \
  -f "https://raw.githubusercontent.com/keycloak/keycloak-operator/${KEYCLOAK_OPERATOR_VERSION}/deploy/operator.yaml" \

kubectl -n "$DROGUE_NS" wait deployment keycloak-operator --for=condition=Available --timeout=-1s
