#!/usr/bin/env bash

set -e

: "${NGINX_VERSION:=3.15.2}"

echo "Installing NGINX Ingress Controller: ${NGINX_VERSION}"

progress -n "  🏗 Deploying operator ... "
kubectl apply -f https://raw.githubusercontent.com/kubernetes/ingress-nginx/ingress-nginx-${NGINX_VERSION}/deploy/static/provider/kind/deploy.yaml
progress "done!"

if [[ "$MINIMIZE" == true ]]; then
    kubectl -n ingress-nginx set resources deployment ingress-nginx-controller --requests=cpu=0
fi

progress -n "  ⏳ Waiting for the operator to become ready ... "
kubectl wait --namespace ingress-nginx --for=condition=ready pod --selector=app.kubernetes.io/component=controller
progress "done!"
