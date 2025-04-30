#!/bin/bash
# Parse the last part of a vcpkg log, printing out the contents of any log files
# suggested by vcpkg to read.

cat $1 \
   | tail -n60 \
   | grep -B 30 -E "error: building .* failed with" \
   | grep -A 20 "See logs for more information:" \
   | grep -E '\-(out|err).log' \
   | xargs -n1 $SHELL -c 'echo "Contents of file $1:"; cat $1' _