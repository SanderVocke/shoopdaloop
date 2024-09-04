#!/bin/bash

# Check if an application is provided
if [ $# -lt 1 ]; then
    echo "Usage: $0 /path/to/executable [args...]"
    echo "       this script expects the executable to terminate by itself."
    exit 1
fi

# Get the executable path
COMMAND=$@

# Temporary files
TRACE_OUTPUT=$(mktemp)
DEPENDENCIES=$(mktemp)

echo "Running \"$COMMAND\". Will list dynamically loaded libraries once execution finishes." >&2
echo "Note that all command output will be redirected to stderr so that stdout only contains the resulting libraries." >&2
echo "" >&2

# Run strace to capture the dynamic library loads
strace_out=$(mktemp)
strace -e openat -o $TRACE_OUTPUT -f $COMMAND 1>&2
cat $TRACE_OUTPUT | grep -E '.*\.so' | grep -v ENOENT | awk -F '"' '{print $2}' | sort -u > "$DEPENDENCIES"

# Output the results
echo "" >&2
echo "Dynamically loaded libraries:" >&2
cat "$DEPENDENCIES"

# Cleanup
rm -f "$TRACE_OUTPUT" "$DEPENDENCIES"
