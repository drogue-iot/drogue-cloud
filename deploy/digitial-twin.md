# Set up digital twin component

## Obtain and configure Vorto secret

    kubectl create secret generic vorto-api --from-literal=token=<my-token>

## Deploy

    kubectl apply -f deploy/02-deploy/07-digital-twin
