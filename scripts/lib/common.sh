#
# This is the central location defining which cluster type we use.
#
# During the creation of the installer, the default of this will be overridden.
#
: "${__DEFAULT_CLUSTER:=minikube}"
: "${CLUSTER:=${__DEFAULT_CLUSTER}}"

: "${DROGUE_NS:=drogue-iot}"
: "${CONTAINER:=docker}"
: "${TEST_CERTS_IMAGE:=ghcr.io/drogue-iot/test-cert-generator:latest}"

#
# Exit with error
#

die() {
    echo "$*" 1>&2
    exit 1
}

bold() {
    tput bold || :
    echo "$@"
    tput sgr0 || :
}

progress() {
    echo "$@" >&3
    echo "$@" >> "$LOG"
}

is_default_cluster() {
    test "$__DEFAULT_CLUSTER" == "$CLUSTER"
}
