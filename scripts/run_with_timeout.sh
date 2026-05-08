#!/usr/bin/env bash
# run_with_timeout.sh - Run a command with a timeout, reliably killing the entire process tree.
#
# Usage: run_with_timeout.sh <timeout_seconds> <command...>
#
# This script addresses issues with the standard `timeout` command:
# - Process trees may hang during exit (especially on macOS/Windows)
# - Child processes may not propagate signals
# - No visibility into when/why a process is killed
# - GitHub Actions runner can hang waiting for orphaned child processes
#
# This script:
# - Runs the command in its own process session (setsid)
# - On timeout OR normal exit, kills ALL descendant processes
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

# Detect platform
IS_WINDOWS=0
if [[ "$(uname -s)" == *"MINGW"* ]] || [[ "$(uname -s)" == *"MSYS"* ]] || [[ "$(uname -s)" == *"CYGWIN"* ]]; then
    IS_WINDOWS=1
fi

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

# Function to find processes by process group
find_by_pgid() {
    local pgid="$1"
    if [[ $IS_WINDOWS -eq 1 ]]; then
        return
    fi
    ps -o pid= -g "$pgid" 2>/dev/null | tr -d ' ' || true
}

# Function to find processes by session
find_by_sid() {
    local sid="$1"
    if [[ $IS_WINDOWS -eq 1 ]]; then
        return
    fi
    ps -o pid= -s "$sid" 2>/dev/null | tr -d ' ' || true
}

# Function to find children of a specific PID
find_children() {
    local parent_pid="$1"
    
    if [[ $IS_WINDOWS -eq 1 ]]; then
        # Windows: use ps in Git Bash
        ps -W 2>/dev/null | awk -v ppid="$parent_pid" 'NR>1 && $2 == ppid {print $1}' || true
    else
        if command -v pgrep &>/dev/null; then
            pgrep -P "$parent_pid" 2>/dev/null || true
        else
            ps -o pid=,ppid= 2>/dev/null | awk -v ppid="$parent_pid" '$2 == ppid {print $1}' || true
        fi
    fi
}

# Function to find all descendants recursively
find_descendants() {
    local parent_pid="$1"
    local descendants=""
    
    local children=""
    children=$(find_children "$parent_pid")
    
    for pid in $children; do
        descendants="$descendants $pid"
        # Recurse
        local grandchildren=""
        grandchildren=$(find_descendants "$pid")
        descendants="$descendants $grandchildren"
    done
    
    echo "$descendants"
}

# Function to kill process tree using stored pgid/sid
kill_process_tree() {
    local target_pid="$1"
    local stored_pgid="$2"
    local stored_sid="$3"
    local signal="$4"
    local signal_name="$5"
    
    echo "[run_with_timeout] Sending ${signal_name} to process tree..."
    
    # Kill by process group (most reliable for orphans)
    if [[ -n "$stored_pgid" ]] && [[ "$stored_pgid" != "" ]]; then
        echo "[run_with_timeout] Killing process group $stored_pgid with ${signal_name}"
        kill "-${signal}" "-${stored_pgid}" 2>/dev/null || true
    fi
    
    # Kill by session
    if [[ -n "$stored_sid" ]] && [[ "$stored_sid" != "" ]] && [[ "$stored_sid" != "$$" ]]; then
        echo "[run_with_timeout] Killing session $stored_sid with ${signal_name}"
        kill "-${signal}" "-${stored_sid}" 2>/dev/null || true
    fi
    
    # Find and kill descendants individually
    local descendants=""
    descendants=$(find_descendants "$target_pid")
    if [[ -n "$descendants" ]]; then
        echo "[run_with_timeout] Killing descendants with ${signal_name}: $descendants"
        for pid in $descendants; do
            kill "-${signal}" "$pid" 2>/dev/null || true
        done
    fi
    
    # Kill the target itself
    kill "-${signal}" "$target_pid" 2>/dev/null || true
}

# Function to wait for process to exit
wait_for_process() {
    local target_pid="$1"
    local wait_seconds="$2"
    local count=0
    
    while kill -0 "$target_pid" 2>/dev/null; do
        if [[ $count -ge $wait_seconds ]]; then
            return 1
        fi
        sleep 1
        ((count++))
    done
    return 0
}

# Function to aggressively clean up all remaining processes
# Uses stored pgid/sid since the main child may have already exited
cleanup_all_processes() {
    local stored_pgid="$1"
    local stored_sid="$2"
    local aggressive="$3"  # "true" to skip SIGTERM and go straight to SIGKILL
    
    echo "[run_with_timeout] Performing aggressive cleanup (pgid=$stored_pgid, sid=$stored_sid, aggressive=${aggressive})..."
    
    # Multiple passes to catch processes that might spawn children during cleanup
    for pass in 1 2 3 4 5; do
        local all_procs=""
        
        # Find by process group
        if [[ -n "$stored_pgid" ]] && [[ "$stored_pgid" != "" ]]; then
            local pgid_procs=""
            pgid_procs=$(find_by_pgid "$stored_pgid")
            all_procs="$all_procs $pgid_procs"
        fi
        
        # Find by session
        if [[ -n "$stored_sid" ]] && [[ "$stored_sid" != "" ]]; then
            local sid_procs=""
            sid_procs=$(find_by_sid "$stored_sid")
            all_procs="$all_procs $sid_procs"
        fi
        
        # Remove duplicates and empty entries, exclude our own PID
        all_procs=$(echo "$all_procs" | tr ' ' '\n' | sort -u | grep -v '^$' | grep -v "^$$\$" | tr '\n' ' ') || true
        
        if [[ -z "$all_procs" ]]; then
            echo "[run_with_timeout] Cleanup pass $pass: no remaining processes"
            break
        fi
        
        echo "[run_with_timeout] Cleanup pass $pass: found processes: $all_procs"
        
        # If aggressive mode, go straight to SIGKILL
        if [[ "$aggressive" == "true" ]]; then
            for pid in $all_procs; do
                echo "[run_with_timeout] SIGKILL PID $pid"
                kill -9 "$pid" 2>/dev/null || true
            done
        else
            # SIGTERM first
            for pid in $all_procs; do
                echo "[run_with_timeout] SIGTERM PID $pid"
                kill -15 "$pid" 2>/dev/null || true
            done
            sleep 2
            
            # SIGKILL any remaining
            all_procs=$(find_by_pgid "$stored_pgid")
            all_procs="$all_procs $(find_by_sid "$stored_sid")"
            all_procs=$(echo "$all_procs" | tr ' ' '\n' | sort -u | grep -v '^$' | grep -v "^$$\$" | tr '\n' ' ') || true
            for pid in $all_procs; do
                echo "[run_with_timeout] SIGKILL PID $pid"
                kill -9 "$pid" 2>/dev/null || true
            done
        fi
        
        sleep 1
    done
    
    # Reap any zombie children
    wait -n 2>/dev/null || true
    
    echo "[run_with_timeout] Cleanup complete"
}

# Kill known problematic processes by name
kill_known_problem_processes() {
    echo "[run_with_timeout] Checking for known problematic processes..."
    for proc_name in tracy-capture crashpad_handler breakpad_handler; do
        if command -v pkill &>/dev/null; then
            pkill -9 -x "$proc_name" 2>/dev/null || true
        elif command -v killall &>/dev/null; then
            killall -9 "$proc_name" 2>/dev/null || true
        fi
    done
}

# Start the command in a new session using setsid
# This creates a new session and process group, making cleanup more reliable
CHILD_PID=""

if command -v setsid &>/dev/null; then
    # Use setsid to create a new session (most reliable)
    setsid "${COMMAND[@]}" &
    CHILD_PID=$!
else
    # Fallback: use job control
    set -m  # Enable job control
    "${COMMAND[@]}" &
    CHILD_PID=$!
    set +m
fi

echo "[run_with_timeout] Child PID: $CHILD_PID"

# CRITICAL: Store the session ID and process group immediately, before the child exits
# These are needed for cleanup when the child is no longer running
STORED_SID=""
STORED_Pgid=""

if [[ $IS_WINDOWS -eq 0 ]]; then
    # Wait a tiny bit for the process to fully start
    sleep 0.1
    STORED_SID=$(ps -o sid= -p $CHILD_PID 2>/dev/null | tr -d ' ') || true
    STORED_Pgid=$(ps -o pgid= -p $CHILD_PID 2>/dev/null | tr -d ' ') || true
    echo "[run_with_timeout] Stored Session: ${STORED_SID:-unknown}, Stored Process Group: ${STORED_Pgid:-unknown}"
fi

# Track elapsed time
START_TIME=$(date +%s)
CHILD_EXIT_CODE=0
LAST_LOG_TIME=0
LOG_INTERVAL=300  # Log every 5 minutes

# Wait for child with timeout
while true; do
    # Check if child is still running
    if ! kill -0 $CHILD_PID 2>/dev/null; then
        # Child has exited, get exit code
        wait $CHILD_PID 2>/dev/null || CHILD_EXIT_CODE=$?
        
        # CRITICAL for GitHub Actions: even after normal exit, orphaned child processes
        # (tracy-capture, crash handler) can still hold stdout/stderr open, causing the
        # tee wrapper to hang indefinitely. We MUST kill them ALL before exiting.
        echo "[run_with_timeout] Main process exited (code $CHILD_EXIT_CODE), cleaning up orphaned child processes..."
        
        # Use stored pgid/sid for cleanup (child is already gone)
        cleanup_all_processes "$STORED_Pgid" "$STORED_SID" "true"
        
        # Kill known problematic processes by name
        kill_known_problem_processes
        
        echo "[run_with_timeout] Orphan cleanup complete, proceeding to exit"
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
        
        # Send SIGTERM first (graceful)
        kill_process_tree "$CHILD_PID" "$STORED_Pgid" "$STORED_SID" "15" "SIGTERM"
        
        # Wait up to 10 seconds for graceful exit
        if wait_for_process "$CHILD_PID" 10; then
            echo "[run_with_timeout] Process exited gracefully after SIGTERM"
            CHILD_EXIT_CODE=124  # Standard timeout exit code
        else
            echo "[run_with_timeout] Process still running after SIGTERM, sending SIGKILL"
            
            # Send SIGKILL (forceful)
            kill_process_tree "$CHILD_PID" "$STORED_Pgid" "$STORED_SID" "9" "SIGKILL"
            
            # Wait up to 5 seconds for forced exit
            if wait_for_process "$CHILD_PID" 5; then
                echo "[run_with_timeout] Process terminated after SIGKILL"
            else
                echo "[run_with_timeout] WARNING: Process may still be running!"
            fi
            CHILD_EXIT_CODE=124
        fi
        
        # Final cleanup using stored pgid/sid
        cleanup_all_processes "$STORED_Pgid" "$STORED_SID" "true"
        kill_known_problem_processes
        
        break
    fi
    
    # Short sleep before checking again
    sleep 1
done

ELAPSED_FINAL=$(($(date +%s) - START_TIME))

# Log final status BEFORE closing file descriptors
# This is the last message that will go through the tee pipe
echo "[run_with_timeout] Command finished with exit code $CHILD_EXIT_CODE after ${ELAPSED_FINAL}s"

# CRITICAL: Close stdout/stderr to signal to the tee wrapper that we're done
# Any remaining output goes to a log file instead
exec 1>> /tmp/run_with_timeout_${CHILD_PID}.log 2>&1

echo "[run_with_timeout] Stdout/stderr closed, performing final cleanup..."

# Flush any remaining output
sync 2>/dev/null || true

# Small delay to ensure previous messages are flushed through tee
sleep 0.5

# Final orphan check (messages go to log file now)
cleanup_all_processes "$STORED_Pgid" "$STORED_SID" "true"
kill_known_problem_processes

echo "[run_with_timeout] Final cleanup done, exiting with code $CHILD_EXIT_CODE"

exit $CHILD_EXIT_CODE