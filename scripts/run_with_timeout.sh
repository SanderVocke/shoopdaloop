#!/usr/bin/env bash
# run_with_timeout.sh - Run a command with a timeout, reliably killing the entire process tree.
#
# Usage: run_with_timeout.sh <timeout_seconds> <command...>
#
# This script addresses issues with the standard `timeout` command:
# - Process trees may hang during exit (especially on macOS/Windows)
# - Child processes may not propagate signals
# - No visibility into when/why a process is killed
#
# This script:
# - Runs the command in its own process group
# - On timeout, kills the entire process tree (SIGTERM then SIGKILL)
# - Logs clearly when timeout occurs and killing is attempted
# - Works on Linux, macOS, and Windows (Git Bash)

set -euo pipefail

TIMEOUT_SECONDS="$1"
shift
COMMAND=("$@")

if [[ -z "${TIMEOUT_SECONDS}" ]] || [[ ${#COMMAND[@]} -eq 0 ]]; then
    echo "Usage: $0 <timeout_seconds> <command...>" >&2
    exit 1
fi

echo "[run_with_timeout] Starting command with ${TIMEOUT_SECONDS}s timeout: ${COMMAND[*]}"
echo "[run_with_timeout] PID: $$"

# Function to kill process tree (process group)
kill_process_tree() {
    local pgid="$1"
    local signal="$2"
    local signal_name="$3"
    
    echo "[run_with_timeout] TIMEOUT EXCEEDED - Sending ${signal_name} to process group ${pgid}"
    
    # Kill the entire process group
    # Use -- to signal the process group (negative PID)
    kill "-${signal}" "-${pgid}" 2>/dev/null || true
}

# Function to wait for process group to exit
wait_for_exit() {
    local pgid="$1"
    local wait_seconds="$2"
    local count=0
    
    while kill -0 "-${pgid}" 2>/dev/null; do
        if [[ $count -ge $wait_seconds ]]; then
            return 1
        fi
        sleep 1
        ((count++))
    done
    return 0
}

# Run the command in a new process group (job control)
# This ensures all child processes are in the same process group
set -m  # Enable job control

# Start the command in background
"${COMMAND[@]}" &
CHILD_PID=$!
CHILD_Pgid=$(ps -o pgid= -p $CHILD_PID 2>/dev/null | tr -d ' ' || echo $CHILD_PID)

# For macOS/BSD, pgid might need a different approach
if [[ -z "$CHILD_Pgid" ]] || [[ "$CHILD_Pgid" == "" ]]; then
    CHILD_Pgid=$CHILD_PID
fi

echo "[run_with_timeout] Child PID: $CHILD_PID, Process Group: $CHILD_Pgid"

# Disable job control messages
set +m

# Track elapsed time
START_TIME=$(date +%s)
CHILD_EXIT_CODE=0
LAST_LOG_TIME=0
LOG_INTERVAL=300  # Log every 5 minutes

# Helper function to format seconds as human-readable
format_time() {
    local seconds=$1
    local mins=$((seconds / 60))
    local secs=$((seconds % 60))
    if [[ $mins -ge 60 ]]; then
        local hours=$((mins / 60))
        local remain_mins=$((mins % 60))
        echo "${hours}h ${remain_mins}m ${secs}s"
    else
        echo "${mins}m ${secs}s"
    fi
}

# Wait for child with timeout
while true; do
    # Check if child is still running
    if ! kill -0 $CHILD_PID 2>/dev/null; then
        # Child has exited, get exit code
        wait $CHILD_PID || CHILD_EXIT_CODE=$?
        break
    fi
    
    # Check timeout
    CURRENT_TIME=$(date +%s)
    ELAPSED=$((CURRENT_TIME - START_TIME))
    
    # Log progress at intervals
    if [[ $((ELAPSED - LAST_LOG_TIME)) -ge $LOG_INTERVAL ]]; then
        LAST_LOG_TIME=$ELAPSED
        REMAINING=$((TIMEOUT_SECONDS - ELAPSED))
        PERCENT=$((ELAPSED * 100 / TIMEOUT_SECONDS))
        echo "[run_with_timeout] Progress: $(format_time $ELAPSED) elapsed, $(format_time $REMAINING) remaining, $(format_time $TIMEOUT_SECONDS) total (${PERCENT}%)"
    fi
    
    if [[ $ELAPSED -ge $TIMEOUT_SECONDS ]]; then
        echo "[run_with_timeout] TIMEOUT: ${TIMEOUT_SECONDS}s exceeded after ${ELAPSED}s elapsed"
        
        # Try to get the process group ID more reliably
        CHILD_Pgid=$(ps -o pgid= -p $CHILD_PID 2>/dev/null | tr -d ' ' || echo "$CHILD_PID")
        
        # Send SIGTERM first (graceful)
        kill_process_tree "$CHILD_Pgid" "15" "SIGTERM"
        
        # Wait up to 10 seconds for graceful exit
        if wait_for_exit "$CHILD_Pgid" 10; then
            echo "[run_with_timeout] Process group exited gracefully after SIGTERM"
            CHILD_EXIT_CODE=124  # Standard timeout exit code
            break
        fi
        
        echo "[run_with_timeout] Process still running after SIGTERM, sending SIGKILL"
        
        # Send SIGKILL (forceful)
        kill_process_tree "$CHILD_Pgid" "9" "SIGKILL"
        
        # Wait up to 5 seconds for forced exit
        if wait_for_exit "$CHILD_Pgid" 5; then
            echo "[run_with_timeout] Process group terminated after SIGKILL"
        else
            echo "[run_with_timeout] WARNING: Process group may still have running processes!"
        fi
        
        CHILD_EXIT_CODE=124  # Standard timeout exit code
        break
    fi
    
    # Short sleep before checking again
    sleep 1
done

ELAPSED_FINAL=$(($(date +%s) - START_TIME))
echo "[run_with_timeout] Command finished with exit code $CHILD_EXIT_CODE after ${ELAPSED_FINAL}s"

exit $CHILD_EXIT_CODE