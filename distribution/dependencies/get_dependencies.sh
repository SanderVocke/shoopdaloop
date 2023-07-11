#!/bin/bash

# Run as: get_dependencies.sh <dependencies_file>
# will take the file <dependencies_file>, remove comments and whitespace,
# and print the rest of the lines as a single line (newlines removed).
# Result is the list of dependencies as can be used in e.g. a single apt-get command.

SCRIPTPATH="$( cd -- "$(dirname "$0")" >/dev/null 2>&1 ; pwd -P )"

cat $SCRIPTPATH/$1 | grep . | grep -E -v '^[\s]*#' | tr '\n' ' '