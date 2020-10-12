#!/usr/bin/env bash

TARGET=${USER:-world}

cat <<EOF | kubectl apply -f -
apiVersion: serving.knative.dev/v1
kind: Service
metadata:
  name: hello
spec:
  template:
    spec:
      containers:
        - image: gcr.io/knative-samples/helloworld-go
          env:
            - name: TARGET
              value: $TARGET
EOF
kubectl wait ksvc hello --all --timeout=-1s --for=condition=Ready
URL=$(kubectl get ksvc hello -o jsonpath='{.status.url}')
echo "URL: $URL"
curl $URL
