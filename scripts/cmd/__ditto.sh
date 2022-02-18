#!/usr/bin/env bash

#
# Install the keycloak operator
#

set -e

: "${DITTO_OPERATOR_HELM_VERSION:=0.4.2}"
: "${DITTO_CRDS:=true}"
: "${DITTO_OPERATOR_HELM_REPO:=https://ctron.github.io/helm-charts}"

progress "📦 Deploying pre-requisites (Ditto Operator v${DITTO_OPERATOR_HELM_VERSION}) ... "

#
# To directly install the Helm chart from the GitHub repository install the helm-git addon:
#
#    helm plugin install https://github.com/aslafy-z/helm-git --version 0.11.1
#
# And then use:
#
#    DITTO_OPERATOR_HELM_REPO=git+https://github.com/ctron/ditto-operator@helm?ref=main
#

if [ "$INSTALL_DITTO_OPERATOR" == true ]; then
    progress -n "  🏗 Deploying the operator ... "

    if [[ "$DITTO_CRDS" == false ]]; then
        HELM_ARGS_DITTO="$HELM_ARGS_DITTO --skip-crds"
    fi

    # shellcheck disable=SC2086
    helm upgrade --install \
        --wait --timeout ${HELM_TIMEOUT} \
        --repo "${DITTO_OPERATOR_HELM_REPO}" \
        ditto-operator ditto-operator \
        --version "${DITTO_OPERATOR_HELM_VERSION}" \
        -n "${DROGUE_NS}" \
        $HELM_ARGS_DITTO \
    > /dev/null

    progress "OK"
fi
