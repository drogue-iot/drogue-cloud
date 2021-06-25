#!/usr/bin/env bash

#
# Check for a set of standard tools we require
#

function check_std_tools() {

    command -v 'kubectl' &>/dev/null || die "Missing the command 'kubectl'"
    command -v 'curl' &>/dev/null || die "Missing the command 'curl'"
    command -v 'sed' &>/dev/null || die "Missing the command 'sed'"
    command -v 'docker' &>/dev/null || command -v 'podman' &>/dev/null || die "Missing the command 'docker' or 'podman'"
    command -v 'helm' &>/dev/null || die "Missing the command 'helm'"

}
