#!/bin/bash

# this script tests whether an application is able to start and stop cleanly.

CMD="$@"
N_ITER=${N_ITERATIONS:-1}
WAIT_MS=${WAIT_MS_BEFORE_CLOSE:-100}
EXPECT_OPEN=${EXPECT_STAY_OPEN:-0}
EXIT_WAIT_MS=${WAIT_MS_BEFORE_HARDKILL:-2000}

echo "Testing: \"$CMD\""
echo "N iterations (N_ITERATIONS): ${N_ITERATIONS:-1}"
echo "Wait before close (WAIT_MS_BEFORE_CLOSE): ${WAIT_MS}"
echo "Wait before hard kill after close (WAIT_MS_BEFORE_KILL): ${EXIT_WAIT_MS}"
echo "Expect app to stay open (EXPECT_STAY_OPEN): $([ $EXPECT_OPEN -ne 0 ] && echo "no" || echo "yes")"

let N_UNEXPECTED_OPEN=0
let N_UNEXPECTED_CLOSED=0
let N_HAD_TO_KILL=0
let N_NONZERO_EXIT=0

for i in $(seq ${N_ITERATIONS:-1}); do
    echo ""
    echo "========================================================="
    echo "RUN ${i}"
    echo "========================================================="
    $CMD &
    PID=$!

    # Wait for milliseconds before closing
    start=$(($(date +%s%N)/1000000))
    while [[ $(( $(($(date +%s%N)/1000000)) - $start )) -lt ${WAIT_MS} ]]; do
        if [[ -z "$(ps | grep " $PID")" ]]; then
            break;
        fi
        sleep 0.01
    done

    if [[ ! -z "$(ps | grep " $PID")" ]]; then
        # still running
        if [ $EXPECT_OPEN -eq 0 ]; then
            echo "=== test_run_app ERROR: app is still open unexpectedly."
            let N_UNEXPECTED_OPEN++
        fi
        
        # kill it with SIGTERM
        echo "=== test_run_app: sending SIGQUIT"
        kill -3 $PID
    else
        if [[ $EXPECT_OPEN -ne 0 ]]; then
            echo "=== test_run_app ERROR: app is already closed unexpectedly."
            let N_UNEXPECTED_CLOSED++
        fi
    fi

    # App is now closed or closing.
    start=$(($(date +%s%N)/1000000))
    while [[ $(( $(($(date +%s%N)/1000000)) - $start )) -lt ${EXIT_WAIT_MS} ]]; do
        if [[ -z "$(ps | grep " $PID")" ]]; then
            break;
        fi
        sleep 0.1
    done

    if [[ ! -z "$(ps | grep " $PID")" ]]; then
        # still running after grace period
        echo "=== test_run_app ERROR: app won't close gracefully; sending SIGKILL"
        let N_HAD_TO_KILL++
        kill -9 $PID
    fi
    wait $PID
    EXITCODE=$?
    if [[ "$EXITCODE" != 0 ]]; then
        echo "=== test_run_app ERROR: nonzero exit code $EXITCODE"
        let N_NONZERO_EXIT++
    fi
done

if [ $N_NONZERO_EXIT -eq 0 ] && [ $N_HAD_TO_KILL -eq 0 ] && [ $N_UNEXPECTED_OPEN -eq 0 ] && [ $N_UNEXPECTED_CLOSED -eq 0 ]; then
    echo ""
    echo "============================================"
    echo "test_run_app: All OK"
    echo "============================================"
    exit 0
else
    echo ""
    echo "============================================"
    echo "test_run_app: FAIL:"
    echo "  - App was unexpectedly open: ${N_UNEXPECTED_OPEN}x"
    echo "  - App was unexpectedly closed: ${N_UNEXPECTED_CLOSED}x"
    echo "  - Had to kill with SIGKILL: ${N_HAD_TO_KILL}x"
    echo "  - Non-zero exit code: ${N_NONZERO_EXIT}x"
    echo "============================================"
    exit 1
fi