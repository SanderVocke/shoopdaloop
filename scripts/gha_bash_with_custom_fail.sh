#!/bin/bash
AND_RESULT=true

if [ "$failure_status" != "" ]; then
  if [ -v GHA_RUN_ALWAYS ]; then
    echo "failure_status is set, but GHA_RUN_ALWAYS is set. Running command."
    AND_RESULT=false # ensure that the end result fails
  else
    echo "failure_status is set"
    exit 1
  fi
fi

echo "GHA wrapper: running command: $BASE_WRAP_SHELL $@"
$BASE_WRAP_SHELL $@ | tee -a log_all.txt
RESULT=${PIPESTATUS[0]} && ${AND_RESULT}

# check if command was successful
if [ ${RESULT} -ne 0 ]; then
  echo "Command failed with exit code $?, setting status to failed"
  echo "failure_status=unhandled" | tee -a $GITHUB_ENV
  echo "ACTION_TMATE_CONNECT_TIMEOUT_SECONDS=3600" | tee -a $GITHUB_ENV
fi