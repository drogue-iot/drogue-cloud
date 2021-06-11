# Get the application domain
function detect_domain() {
    if [[ -n "$DOMAIN" ]]; then
        echo "$DOMAIN"
        return
    fi

    local domain
    case $CLUSTER in
    kind)
        domain=$(kubectl get node kind-control-plane -o jsonpath='{.status.addresses[?(@.type == "InternalIP")].address}').nip.io
        ;;
    minikube)
        domain=$(minikube ip).nip.io
        ;;
    openshift)
        domain=$(kubectl -n openshift-ingress-operator get ingresscontrollers.operator.openshift.io default -o jsonpath='{.status.domain}')
        ;;
    *)
        die "Unable to auto-detect DNS domain on Kubernetes platform '$CLUSTER': Use -d or set DOMAIN to the base DNS domain."
        ;;
    esac
    echo "$domain"
}

function service_url() {
    local name="$1"
    shift
    local scheme="$1"

    case $CLUSTER in
    kubernetes)
        DOMAIN=$(kubectl get service -n "$DROGUE_NS" "$name" -o 'jsonpath={ .status.loadBalancer.ingress[0].ip }').nip.io
        PORT=$(kubectl get service -n "$DROGUE_NS" "$name" -o jsonpath='{.spec.ports[0].port}')
        URL=${scheme:-http}://$name.$DOMAIN:$PORT
        ;;
    kind)
        DOMAIN=$(domain)
        PORT=$(kubectl get service -n "$DROGUE_NS" "$name" -o jsonpath='{.spec.ports[0].nodePort}')
        URL=${scheme:-http}://$name.$DOMAIN:$PORT
        ;;
    minikube)
        test -n "$scheme" && scheme="--$scheme"
        URL=$(eval minikube service -n "$DROGUE_NS" $scheme --url "$name")
        ;;
    openshift)
        URL="https://$(kubectl get route -n "$DROGUE_NS" "$name" -o 'jsonpath={ .spec.host }')"
        ;;
    *)
        die "Unknown Kubernetes platform: $CLUSTER ... unable to extract endpoints"
        ;;
    esac
    echo "$URL"
}

function route_url() {
    local name="$1"
    shift

    case $CLUSTER in
    openshift)
        URL="$(kubectl get route -n "$DROGUE_NS" "$name" -o 'jsonpath={ .spec.host }')"
        if [ -n "$URL" ]; then
            URL="https://$URL"
        fi
        ;;
    *)
        ingress_url "$name"
        ;;
    esac
}

function ingress_url() {
    local name="$1"
    shift

    local DOMAIN
    local URL
    local HOST
    local PROTO

    case $CLUSTER in
    openshift)
        PROTO=https
        ;;
    *)
        PROTO=http
        ;;
    esac

    HOST=$(kubectl get ingress -n "$DROGUE_NS" "$name" -o 'jsonpath={ .spec.rules[0].host }')
    if [ -n "$HOST" ]; then
        URL="${PROTO}://${HOST}"
    fi
    echo "$URL"
}

function ingress_url_wait() {
    local name
    name=$1
    shift

    local URL
    URL="$(ingress_url "$name")"
    while [ -z "$URL" ]; do

        if [[ "$CLUSTER" == "minikube" ]]; then
            progress "🔌 Waiting for ingress ($name) to get ready! If you're running minikube, run 'minikube tunnel' in another shell and ensure that you have the ingress addon enabled."
        else
            progress "🔌 Waiting for ingress ($name) to get ready!"
        fi

        sleep 5
        URL="$(ingress_url "$name")"
    done

    echo "$URL"
}
