#!/bin/bash
SCRIPTPATH="$( cd -- "$(dirname "$0")" >/dev/null 2>&1 ; pwd -P )"

cat $SCRIPTPATH/$1 | grep . | grep -E -v '^[\s]*#' | tr '\n' ' '