#!/bin/bash

if [ "$failure_status" != "" ]; then
  if [ -v GHA_RUN_ALWAYS ]; then
    echo "failure_status is set, but GHA_RUN_ALWAYS is set. Running command."
  else
    echo "failure_status is set"
    exit 1
  fi
fi

echo "GHA wrapper: running command: $BASE_WRAP_SHELL $@"
$BASE_WRAP_SHELL $@

# check if command was successful
if [ $? -ne 0 ]; then
  echo "Command failed with exit code $?, setting status to failed"
  echo "failure_status=unhandled" | tee -a $GITHUB_ENV
fi