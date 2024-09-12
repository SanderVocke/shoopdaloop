#!/bin/bash

# Check if an application is provided
if [ $# -lt 1 ]; then
    echo "Usage: $0 /path/to/executable [args...]"
    echo "       this script expects the executable to terminate by itself."
    exit 1
fi

MY_OS=$(uname)
if [[ "$MY_OS" != "Linux" ]] && [[ "$OS" == Windows* ]]; then
    MY_OS=Windows
fi

echo "OS: $MY_OS" >&2

# Get the executable path
COMMAND=$@
EXECUTABLE=$1

# Temporary files
# TRACE_OUTPUT=$(mktemp)
# DEPENDENCIES=$(mktemp)

# echo "Running \"$COMMAND\". Will list dynamically loaded libraries once execution finishes." >&2
# echo "Note that all command output will be redirected to stderr so that stdout only contains the resulting libraries." >&2
# echo "" >&2

if [[ "$MY_OS" == "Linux" ]]; then
    # # Run strace to capture the dynamic library loads
    # strace -e openat -o $TRACE_OUTPUT -f $COMMAND 1>&2
    # RESULT=$?
    # cat $TRACE_OUTPUT | grep -E '.*\.so' | grep -v ENOENT | awk -F '"' '{print $2}' | sort -u > "$DEPENDENCIES"
    ldd $EXECUTABLE | grep "=>" | sed -r 's/.*=>[ ]*([^ ]+) .*/\1/g' | sort | uniq
elif [[ "$MY_OS" == "Darwin" ]]; then
    # lldb -s <(printf "run\nimage list\nq\n") ${COMMAND%% *} -- ${COMMAND#* } | tee -a $TRACE_OUTPUT
    # cat $TRACE_OUTPUT | grep '.dylib' | awk 'NF>1{print $NF}' | sort -u > "$DEPENDENCIES"
    # DYLD_PRINT_LIBRARIES=1 $COMMAND 2>$TRACE_OUTPUT 1>&2
    # RESULT=$?
    # cat $TRACE_OUTPUT | grep 'dyld' | grep '.dylib' | awk 'NF>1{print $NF}' | sort -u > "$DEPENDENCIES"
    otool -L $EXECUTABLE | grep -v ':' | awk '{print $1}' | sort | uniq
elif [[ "$MY_OS" == "Windows" ]]; then
    # Dumpbin
    dumpbin /DEPENDENTS $EXECUTABLE | grep "DLL Name:" | awk '{print $3}' | sort | uniq
fi

# Output the results
# echo "" >&2
# echo "Raw output:" >&2
# cat $TRACE_OUTPUT >&2
# echo "" >&2
# echo "Dynamically loaded libraries:" >&2
# cat "$DEPENDENCIES"

# Cleanup
# rm -f "$TRACE_OUTPUT" "$DEPENDENCIES"
exit $RESULT
