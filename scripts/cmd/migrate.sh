#!/usr/bin/sh

if [ $# -lt 1 ]
then
  echo "Application ID must be supplied. Usage: ./credentials-migrate.sh <app-id>"
  exit 1
fi
APPLICATION=$1

which jq > /dev/null && which drg > /dev/null
if [ $? -ne 0 ]
then
  echo "jq and drg not found. These tools are mandatory to run this script"
  exit 1
fi

# test for a valid drg config
CONFIG=$(drg config show --active -o json | jq .name)
if [ $? -ne 0 ]
then
  echo "drg is not logged in. Set up drg before continuing with: drg login <url>"
  exit 1
fi

echo "Using drg config $CONFIG"

# get a list of device | keep only column 1 | remove header
DEVICES_ID=$(drg get devices --app "$APPLICATION" | awk '{print $1}' | tail -n +2)

for dev in $DEVICES_ID
do
  DATA=$(drg get device "$dev" -o json --app "$APPLICATION")
  hascreds=$(echo "$DATA" | jq '.spec | has("credentials")')

  if [ "$hascreds" = "true" ]
  then
    DATA=$(echo "$DATA" | jq '.spec.authentication += .spec.credentials')
    echo "$DATA" | drg apply -f -
    progress "Device $dev updated."
  else
      progress "No credentials field for device $dev"
  fi

done
