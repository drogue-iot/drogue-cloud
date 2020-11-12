#!/usr/bin/env bash

set -ex

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
source "$SCRIPTDIR/common.sh"

JWT_KEY="$SCRIPTDIR/../tmp/jwt.key"
mkdir -p "$(dirname "$JWT_KEY")"
if ! test -f "$JWT_KEY" ; then
  echo "Creating JWT key..."
  ssh-keygen -t ecdsa -b 256 -m PKCS8 -f "$JWT_KEY" -N ""
  kubectl create secret generic jwt-key --from-file=jwt.key="$JWT_KEY"
fi
