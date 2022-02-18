
# check cluster connectivity
function check_cluster_connection() {
    progress -n "🌱 Checking cluster connectivity ... "
    kubectl version &>/dev/null || die "Unable to connect to the cluster: 'kubectl' must be able to connect to your cluster."
    progress "done!"
}

# Get the application domain
function detect_domain() {
    if [[ -n "$DOMAIN" ]]; then
        echo "$DOMAIN"
        return
    fi

    local domain
    case $CLUSTER in
    kind)
        domain=.$(kubectl get node kind-control-plane -o jsonpath='{.status.addresses[?(@.type == "InternalIP")].address}' | awk '// { print $1 }').nip.io
        ;;
    minikube)
        domain=.$(minikube ip).nip.io
        ;;
    openshift)
        domain=-${DROGUE_NS}.$(kubectl -n openshift-ingress-operator get ingresscontrollers.operator.openshift.io default -o jsonpath='{.status.domain}')
        ;;
    *)
        die "Unable to auto-detect DNS domain on Kubernetes platform '$CLUSTER': Use -d or set DOMAIN to the base DNS domain."
        ;;
    esac
    echo "$domain"
}
