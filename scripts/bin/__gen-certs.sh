#!/usr/bin/env bash

set -e

SCRIPTDIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"

CERT_ALTNAMES="$1"

test -n "$CERT_ALTNAMES" || die "Missing alt-names argument: ./scripts/gen-certs.sh <alt-names>"
test -n "$CONTAINER" || die "Variable 'CONTAINER' is not set"

: "${OUT:=${SCRIPTDIR}/../build/certs/endpoints}"

rm -Rf "$OUT"
mkdir -p "$OUT"
"${CONTAINER}" run --rm -t -v "$OUT:/etc/drogue-certs:z" -e "EBASE=endpoints/" -e CERT_ALTNAMES="$CERT_ALTNAMES" "${TEST_CERTS_IMAGE}"
