#!/bin/bash

# Check if an application is provided
if [ $# -lt 1 ]; then
    echo "Usage: $0 /path/to/executable [args...]"
    echo "       this script expects the executable to terminate by itself."
    exit 1
fi

OS=$(uname)
if [[ "$OS" != "Linux" ]] && [[ "$(bash.exe -c 'echo hi')" == "hi" ]]; then
    OS=Windows
fi

echo "OS: $OS" >&2

# Get the executable path
COMMAND=$@

# Temporary files
TRACE_OUTPUT=$(mktemp)
DEPENDENCIES=$(mktemp)

echo "Running \"$COMMAND\". Will list dynamically loaded libraries once execution finishes." >&2
echo "Note that all command output will be redirected to stderr so that stdout only contains the resulting libraries." >&2
echo "" >&2

if [[ "$OS" == "Linux" ]]; then
    # Run strace to capture the dynamic library loads
    strace -e openat -o $TRACE_OUTPUT -f $COMMAND 1>&2
    cat $TRACE_OUTPUT | grep -E '.*\.so' | grep -v ENOENT | awk -F '"' '{print $2}' | sort -u > "$DEPENDENCIES"
elif [[ "$OS" == "Darwin" ]]; then
    DYLD_PRINT_LIBRARIES=1 $COMMAND 1>&2
    cat $TRACE_OUTPUT | grep 'dyld' | grep 'loaded' | grep '.dylib' | awk 'NF>1{print $NF}' | sort -u > "$DEPENDENCIES"
fi

# Output the results
echo "" >&2
echo "Dynamically loaded libraries:" >&2
cat "$DEPENDENCIES"

# Cleanup
rm -f "$TRACE_OUTPUT" "$DEPENDENCIES"
