#!/usr/bin/env bash

#
# Install the keycloak operator
#

set -ex

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
source "$SCRIPTDIR/common.sh"

: "${KEYCLOAK_OPERATOR_VERSION:=11.0.3}"
: "${CLUSTER:=minikube}"
: "${KEYCLOAK_CRDS:=true}"

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"

if [[ "$KEYCLOAK_CRDS" == true ]]; then

kubectl apply \
  -f "https://raw.githubusercontent.com/keycloak/keycloak-operator/11.0.3/deploy/crds/keycloak.org_keycloakbackups_crd.yaml" \
  -f "https://raw.githubusercontent.com/keycloak/keycloak-operator/11.0.3/deploy/crds/keycloak.org_keycloakclients_crd.yaml" \
  -f "https://raw.githubusercontent.com/keycloak/keycloak-operator/11.0.3/deploy/crds/keycloak.org_keycloakrealms_crd.yaml" \
  -f "https://raw.githubusercontent.com/keycloak/keycloak-operator/11.0.3/deploy/crds/keycloak.org_keycloaks_crd.yaml" \
  -f "https://raw.githubusercontent.com/keycloak/keycloak-operator/11.0.3/deploy/crds/keycloak.org_keycloakusers_crd.yaml" \

fi

kubectl apply -n "$DROGUE_NS" \
  -f "https://raw.githubusercontent.com/keycloak/keycloak-operator/11.0.3/deploy/service_account.yaml" \
  -f "https://raw.githubusercontent.com/keycloak/keycloak-operator/11.0.3/deploy/role.yaml" \
  -f "https://raw.githubusercontent.com/keycloak/keycloak-operator/11.0.3/deploy/role_binding.yaml" \
  -f "https://raw.githubusercontent.com/keycloak/keycloak-operator/11.0.3/deploy/operator.yaml" \

