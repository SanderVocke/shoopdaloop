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
echo "[run_with_timeout] DEBUG: IS_WINDOWS=$IS_WINDOWS, uname=$(uname -s)"

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
    echo "[run_with_timeout] DEBUG: find_by_pgid(pgid=$pgid)"
    if [[ $IS_WINDOWS -eq 1 ]]; then
        echo "[run_with_timeout] DEBUG:   skipped (Windows)"
        return
    fi
    local result=""
    result=$(ps -o pid= -g "$pgid" 2>/dev/null | tr -d ' ' || true)
    echo "[run_with_timeout] DEBUG:   result='$result'"
    echo "$result"
}

# Function to find processes by session
find_by_sid() {
    local sid="$1"
    echo "[run_with_timeout] DEBUG: find_by_sid(sid=$sid)"
    if [[ $IS_WINDOWS -eq 1 ]]; then
        echo "[run_with_timeout] DEBUG:   skipped (Windows)"
        return
    fi
    local result=""
    result=$(ps -o pid= -s "$sid" 2>/dev/null | tr -d ' ' || true)
    echo "[run_with_timeout] DEBUG:   result='$result'"
    echo "$result"
}

# Function to find children of a specific PID
find_children() {
    local parent_pid="$1"
    echo "[run_with_timeout] DEBUG: find_children(parent_pid=$parent_pid)"
    local result=""
    if [[ $IS_WINDOWS -eq 1 ]]; then
        result=$(powershell -NoProfile -Command "Get-CimInstance Win32_Process -Filter \"ParentProcessId=$parent_pid\" | Select-Object -ExpandProperty ProcessId" 2>/dev/null | tr -d '\r' || true)
    else
        if command -v pgrep &>/dev/null; then
            result=$(pgrep -P "$parent_pid" 2>/dev/null || true)
        else
            result=$(ps -o pid=,ppid= 2>/dev/null | awk -v ppid="$parent_pid" '$2 == ppid {print $1}' || true)
        fi
    fi
    echo "[run_with_timeout] DEBUG:   result='$result'"
    echo "$result"
}

# Function to find all descendants recursively
find_descendants() {
    local parent_pid="$1"
    echo "[run_with_timeout] DEBUG: find_descendants(parent_pid=$parent_pid)"
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
    
    echo "[run_with_timeout] DEBUG:   descendants='$descendants'"
    echo "$descendants"
}

# Function to kill process tree using stored pgid/sid
kill_process_tree() {
    local target_pid="$1"
    local stored_pgid="$2"
    local stored_sid="$3"
    local signal="$4"
    local signal_name="$5"
    
    echo "[run_with_timeout] DEBUG: kill_process_tree(target_pid=$target_pid, pgid=$stored_pgid, sid=$stored_sid, signal=$signal_name)"
    echo "[run_with_timeout] Sending ${signal_name} to process tree..."
    
    if [[ $IS_WINDOWS -eq 1 ]]; then
        echo "[run_with_timeout] DEBUG:   Windows path"
        local descendants=""
        descendants=$(find_descendants "$target_pid")
        if [[ -n "$descendants" ]]; then
            echo "[run_with_timeout] Killing descendants with ${signal_name}: $descendants"
            for pid in $descendants; do
                if [[ "$pid" != "$$" ]]; then
                    echo "[run_with_timeout] DEBUG:   Stop-Process -Id $pid"
                    powershell -NoProfile -Command "Stop-Process -Id $pid -Force -ErrorAction SilentlyContinue" 2>/dev/null || true
                fi
            done
        else
            echo "[run_with_timeout] DEBUG:   no descendants found"
        fi
        if [[ "$target_pid" != "$$" ]]; then
            echo "[run_with_timeout] Killing target PID $target_pid with ${signal_name}"
            echo "[run_with_timeout] DEBUG:   Stop-Process -Id $target_pid"
            powershell -NoProfile -Command "Stop-Process -Id $target_pid -Force -ErrorAction SilentlyContinue" 2>/dev/null || true
        fi
        return
    fi
    
    # Kill by process group (most reliable for orphans)
    if [[ -n "$stored_pgid" ]] && [[ "$stored_pgid" != "" ]]; then
        echo "[run_with_timeout] Killing process group $stored_pgid with ${signal_name}"
        echo "[run_with_timeout] DEBUG:   kill -${signal} -${stored_pgid}"
        kill "-${signal}" "-${stored_pgid}" 2>/dev/null || true
    else
        echo "[run_with_timeout] DEBUG:   no stored pgid, skipping group kill"
    fi
    
    # Kill by session
    if [[ -n "$stored_sid" ]] && [[ "$stored_sid" != "" ]] && [[ "$stored_sid" != "$$" ]]; then
        echo "[run_with_timeout] Killing session $stored_sid with ${signal_name}"
        echo "[run_with_timeout] DEBUG:   kill -${signal} -${stored_sid}"
        kill "-${signal}" "-${stored_sid}" 2>/dev/null || true
    else
        echo "[run_with_timeout] DEBUG:   no stored sid (or sid==$$), skipping session kill"
    fi
    
    # Find and kill descendants individually
    local descendants=""
    descendants=$(find_descendants "$target_pid")
    if [[ -n "$descendants" ]]; then
        echo "[run_with_timeout] Killing descendants with ${signal_name}: $descendants"
        for pid in $descendants; do
            echo "[run_with_timeout] DEBUG:   kill -${signal} $pid"
            kill "-${signal}" "$pid" 2>/dev/null || true
        done
    else
        echo "[run_with_timeout] DEBUG:   no descendants to kill individually"
    fi
    
    # Kill the target itself
    echo "[run_with_timeout] DEBUG:   kill -${signal} $target_pid"
    kill "-${signal}" "$target_pid" 2>/dev/null || true
}

# Function to wait for process to exit
wait_for_process() {
    local target_pid="$1"
    local wait_seconds="$2"
    local count=0
    echo "[run_with_timeout] DEBUG: wait_for_process(pid=$target_pid, max=${wait_seconds}s)"
    
    while kill -0 "$target_pid" 2>/dev/null; do
        if [[ $count -ge $wait_seconds ]]; then
            echo "[run_with_timeout] DEBUG:   timed out after ${count}s"
            return 1
        fi
        sleep 1
        ((count++)) || true
        echo "[run_with_timeout] DEBUG:   still alive after ${count}s"
    done
    echo "[run_with_timeout] DEBUG:   exited after ${count}s"
    return 0
}

# Global variable to track if any processes were killed during cleanup
CLEANUP_KILLED_PROCESSES=0
CLEANUP_KILLED_PIDS=""

# Function to aggressively clean up all remaining processes
# Uses stored pgid/sid since the main child may have already exited
# Sets CLEANUP_KILLED_PROCESSES to count of processes killed
# Sets CLEANUP_KILLED_PIDS to space-separated list of killed PIDs
cleanup_all_processes() {
    local stored_pgid="$1"
    local stored_sid="$2"
    local aggressive="$3"  # "true" to skip SIGTERM and go straight to SIGKILL
    
    echo "[run_with_timeout] Performing aggressive cleanup (pgid=$stored_pgid, sid=$stored_sid, aggressive=${aggressive})..."
    echo "[run_with_timeout] DEBUG: cleanup_all_processes entry: CHILD_PID=${CHILD_PID:-<unset>}, IS_WINDOWS=$IS_WINDOWS"
    
    local total_killed=0
    local killed_pids=""
    
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
        
        # Windows fallback: if no pgid/sid available, target live descendants of the original child
        if [[ $IS_WINDOWS -eq 1 ]] && [[ -n "${CHILD_PID:-}" ]] && kill -0 "$CHILD_PID" 2>/dev/null; then
            echo "[run_with_timeout] DEBUG:   Windows fallback: CHILD_PID=$CHILD_PID is alive, fetching descendants"
            local win_descendants=""
            win_descendants=$(find_descendants "$CHILD_PID")
            all_procs="$all_procs $CHILD_PID $win_descendants"
        elif [[ $IS_WINDOWS -eq 1 ]]; then
            echo "[run_with_timeout] DEBUG:   Windows fallback: CHILD_PID=${CHILD_PID:-<unset>} not alive or not set"
        fi
        
        # Remove duplicates and empty entries, exclude our own PID
        all_procs=$(echo "$all_procs" | tr ' ' '\n' | sort -u | grep -v '^$' | grep -v "^$$\$" | tr '\n' ' ') || true
        echo "[run_with_timeout] DEBUG:   pass $pass deduped list (excluding self): '$all_procs'"
        
        if [[ -z "$all_procs" ]]; then
            echo "[run_with_timeout] Cleanup pass $pass: no remaining processes"
            break
        fi
        
        echo "[run_with_timeout] Cleanup pass $pass: found processes: $all_procs"
        
        # Track how many we kill in this pass
        local pass_killed=0
        
        # If aggressive mode, go straight to SIGKILL
        if [[ "$aggressive" == "true" ]]; then
            for pid in $all_procs; do
                if [[ "$pid" == "$$" ]]; then
                    continue
                fi
                echo "[run_with_timeout] SIGKILL PID $pid"
                if [[ $IS_WINDOWS -eq 1 ]]; then
                    echo "[run_with_timeout] DEBUG:   Stop-Process -Id $pid"
                    powershell -NoProfile -Command "Stop-Process -Id $pid -Force -ErrorAction SilentlyContinue" 2>/dev/null || true
                else
                    echo "[run_with_timeout] DEBUG:   kill -9 $pid"
                    kill -9 "$pid" 2>/dev/null || true
                fi
                ((pass_killed++)) || true
                killed_pids="$killed_pids $pid"
            done
        else
            # SIGTERM first
            for pid in $all_procs; do
                if [[ "$pid" == "$$" ]]; then
                    continue
                fi
                echo "[run_with_timeout] SIGTERM PID $pid"
                if [[ $IS_WINDOWS -eq 1 ]]; then
                    echo "[run_with_timeout] DEBUG:   Stop-Process -Id $pid"
                    powershell -NoProfile -Command "Stop-Process -Id $pid -Force -ErrorAction SilentlyContinue" 2>/dev/null || true
                else
                    echo "[run_with_timeout] DEBUG:   kill -15 $pid"
                    kill -15 "$pid" 2>/dev/null || true
                fi
                ((pass_killed++)) || true
                killed_pids="$killed_pids $pid"
            done
            sleep 2
            
            # SIGKILL any remaining
            all_procs=$(find_by_pgid "$stored_pgid")
            all_procs="$all_procs $(find_by_sid "$stored_sid")"
            if [[ $IS_WINDOWS -eq 1 ]] && [[ -n "${CHILD_PID:-}" ]] && kill -0 "$CHILD_PID" 2>/dev/null; then
                local win_descendants=""
                win_descendants=$(find_descendants "$CHILD_PID")
                all_procs="$all_procs $CHILD_PID $win_descendants"
            fi
            all_procs=$(echo "$all_procs" | tr ' ' '\n' | sort -u | grep -v '^$' | grep -v "^$$\$" | tr '\n' ' ') || true
            echo "[run_with_timeout] DEBUG:   pass $pass post-SIGTERM deduped list: '$all_procs'"
            for pid in $all_procs; do
                if [[ "$pid" == "$$" ]]; then
                    continue
                fi
                echo "[run_with_timeout] SIGKILL PID $pid"
                if [[ $IS_WINDOWS -eq 1 ]]; then
                    echo "[run_with_timeout] DEBUG:   Stop-Process -Id $pid"
                    powershell -NoProfile -Command "Stop-Process -Id $pid -Force -ErrorAction SilentlyContinue" 2>/dev/null || true
                else
                    echo "[run_with_timeout] DEBUG:   kill -9 $pid"
                    kill -9 "$pid" 2>/dev/null || true
                fi
                ((pass_killed++)) || true
                killed_pids="$killed_pids $pid"
            done
        fi
        
        total_killed=$((total_killed + pass_killed))
        echo "[run_with_timeout] DEBUG:   pass $pass killed=$pass_killed, total_killed=$total_killed"
        sleep 1
    done
    
    # Reap any zombie children (use 'wait' instead of 'wait -n' for bash 3.2/macOS compatibility)
    wait 2>/dev/null || true
    
    CLEANUP_KILLED_PROCESSES=$total_killed
    CLEANUP_KILLED_PIDS="$killed_pids"
    
    echo "[run_with_timeout] Cleanup complete (total_killed=$total_killed, pids='$killed_pids')"
}

# Kill known problematic processes by name
# Increments CLEANUP_KILLED_PROCESSES and appends PIDs to CLEANUP_KILLED_PIDS
kill_known_problem_processes() {
    echo "[run_with_timeout] Checking for known problematic processes..."
    # Deduplicate resolved Windows image names to avoid double-counting
    local seen_img_names=""
    for proc_name in tracy-capture crashpad_handler breakpad_handler shoopdaloop shoopdaloop_exe shoopdaloop.exe shoopdaloop_exe.exe; do
        if [[ $IS_WINDOWS -eq 1 ]]; then
            # Build exact executable name for Win32_Process (e.g. "shoopdaloop_exe.exe")
            local img_name="$proc_name"
            if [[ "$img_name" != *.exe ]]; then
                img_name="${img_name}.exe"
            fi
            # Skip duplicates (e.g. shoopdaloop_exe and shoopdaloop_exe.exe both -> shoopdaloop_exe.exe)
            if [[ "$seen_img_names" == *" $img_name "* ]]; then
                echo "[run_with_timeout] DEBUG:   skipping duplicate Windows image name: $img_name"
                continue
            fi
            seen_img_names="$seen_img_names $img_name "
            echo "[run_with_timeout]   Checking Windows image name: $img_name"
            local pids=""
            pids=$(powershell -NoProfile -Command "Get-CimInstance Win32_Process -Filter \"Name='$img_name'\" | Select-Object -ExpandProperty ProcessId" 2>/dev/null | tr -d '\r' || true)
            if [[ -n "$pids" ]]; then
                echo "[run_with_timeout]   Found $img_name PIDs: $(echo "$pids" | tr '\n' ' ')"
                for pid in $pids; do
                    ((CLEANUP_KILLED_PROCESSES++)) || true
                    CLEANUP_KILLED_PIDS="$CLEANUP_KILLED_PIDS $pid"
                done
                # Use taskkill /F /PID for each — with Stop-Process fallback if taskkill fails
                for pid in $pids; do
                    echo "[run_with_timeout]   taskkill /F /PID $pid ($img_name)"
                    local taskkill_out=""
                    local taskkill_rc=0
                    taskkill_out=$(taskkill /F /PID "$pid" 2>&1) || taskkill_rc=$?
                    if [[ $taskkill_rc -eq 0 ]]; then
                        echo "[run_with_timeout]   taskkill succeeded: $taskkill_out"
                    else
                        echo "[run_with_timeout]   taskkill FAILED (rc=$taskkill_rc): $taskkill_out"
                        echo "[run_with_timeout]   trying Stop-Process fallback for PID $pid"
                        powershell -NoProfile -Command "Stop-Process -Id $pid -Force -ErrorAction SilentlyContinue" 2>/dev/null || true
                    fi
                done
            else
                echo "[run_with_timeout]   No matches for $img_name"
            fi
        elif command -v pgrep &>/dev/null; then
            echo "[run_with_timeout] DEBUG:   Unix pgrep path for '$proc_name'"
            local pids=""
            pids=$(pgrep -x "$proc_name" 2>/dev/null || true)
            echo "[run_with_timeout] DEBUG:   pgrep result: '$pids'"
            if [[ -n "$pids" ]]; then
                for pid in $pids; do
                    ((CLEANUP_KILLED_PROCESSES++)) || true
                    CLEANUP_KILLED_PIDS="$CLEANUP_KILLED_PIDS $pid"
                done
                echo "[run_with_timeout] DEBUG:   pkill -9 -x '$proc_name'"
                pkill -9 -x "$proc_name" 2>/dev/null || true
            fi
        elif command -v killall &>/dev/null; then
            echo "[run_with_timeout] DEBUG:   Unix killall path for '$proc_name'"
            # killall doesn't list PIDs; try pgrep for counting, but kill blind if unavailable
            local pids=""
            if command -v pgrep &>/dev/null; then
                pids=$(pgrep -x "$proc_name" 2>/dev/null || true)
                echo "[run_with_timeout] DEBUG:   pgrep result: '$pids'"
            fi
            echo "[run_with_timeout] DEBUG:   killall -9 '$proc_name'"
            killall -9 "$proc_name" 2>/dev/null || true
            if [[ -n "$pids" ]]; then
                for pid in $pids; do
                    ((CLEANUP_KILLED_PROCESSES++)) || true
                    CLEANUP_KILLED_PIDS="$CLEANUP_KILLED_PIDS $pid"
                done
            fi
        else
            echo "[run_with_timeout] DEBUG:   no pkill/killall available, skipping Unix name kill"
        fi
    done
}

# Start the command in a new session using setsid
# This creates a new session and process group, making cleanup more reliable
CHILD_PID=""

if command -v setsid &>/dev/null; then
    echo "[run_with_timeout] DEBUG: launching with setsid"
    setsid "${COMMAND[@]}" &
    CHILD_PID=$!
else
    echo "[run_with_timeout] DEBUG: setsid not available, launching with job control"
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
else
    echo "[run_with_timeout] DEBUG: Windows path: skipping sid/pgid storage (not applicable)"
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
        
        echo "[run_with_timeout] DEBUG: child $CHILD_PID no longer running, raw exit code=$CHILD_EXIT_CODE"
        
        # Small delay to let things settle before cleanup
        echo "[run_with_timeout] Main process exited (code $CHILD_EXIT_CODE), waiting 1s before cleanup..."
        sleep 1
        
        # CRITICAL for GitHub Actions: even after normal exit, orphaned child processes
        # (tracy-capture, crash handler) can still hold stdout/stderr open, causing the
        # tee wrapper to hang indefinitely. We MUST kill them ALL before exiting.
        echo "[run_with_timeout] Cleaning up orphaned child processes..."
        echo "[run_with_timeout] DEBUG: pre-cleanup state: CLEANUP_KILLED_PROCESSES=$CLEANUP_KILLED_PROCESSES"
        
        # Use stored pgid/sid for cleanup (child is already gone)
        cleanup_all_processes "$STORED_Pgid" "$STORED_SID" "true"
        
        echo "[run_with_timeout] DEBUG: post-cleanup_all_processes state: CLEANUP_KILLED_PROCESSES=$CLEANUP_KILLED_PROCESSES, PIDS='$CLEANUP_KILLED_PIDS'"
        
        # Kill known problematic processes by name
        kill_known_problem_processes
        
        echo "[run_with_timeout] DEBUG: post-kill_known_problem_processes state: CLEANUP_KILLED_PROCESSES=$CLEANUP_KILLED_PROCESSES, PIDS='$CLEANUP_KILLED_PIDS'"
        
        # Check if we found any leftover processes - this is an error in CI
        if [[ $CLEANUP_KILLED_PROCESSES -gt 0 ]]; then
            echo "[run_with_timeout] ERROR: Found and killed $CLEANUP_KILLED_PROCESSES leftover process(es):$CLEANUP_KILLED_PIDS"
            echo "[run_with_timeout] ERROR: Leftover processes are considered a failure in CI context."
            echo "[run_with_timeout] ERROR: Please ensure all child processes are properly terminated by the main process."
            CHILD_EXIT_CODE=125  # Special exit code for leftover processes
        fi
        
        echo "[run_with_timeout] Cleanup complete, proceeding to exit (final exit code=$CHILD_EXIT_CODE)"
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
        echo "[run_with_timeout] DEBUG: entering timeout kill path"
        
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
        
        # Small delay before final cleanup
        sleep 1
        
        echo "[run_with_timeout] DEBUG: pre-timeout-cleanup state: CLEANUP_KILLED_PROCESSES=$CLEANUP_KILLED_PROCESSES"
        # Final cleanup using stored pgid/sid
        cleanup_all_processes "$STORED_Pgid" "$STORED_SID" "true"
        echo "[run_with_timeout] DEBUG: post-timeout-cleanup state: CLEANUP_KILLED_PROCESSES=$CLEANUP_KILLED_PROCESSES"
        kill_known_problem_processes
        echo "[run_with_timeout] DEBUG: post-timeout-known-kill state: CLEANUP_KILLED_PROCESSES=$CLEANUP_KILLED_PROCESSES"
        
        # Check if we found any leftover processes - this is an error in CI
        if [[ $CLEANUP_KILLED_PROCESSES -gt 0 ]]; then
            echo "[run_with_timeout] ERROR: Found and killed $CLEANUP_KILLED_PROCESSES leftover process(es):$CLEANUP_KILLED_PIDS"
            echo "[run_with_timeout] ERROR: Leftover processes are considered a failure in CI context."
            echo "[run_with_timeout] ERROR: Please ensure all child processes are properly terminated by the main process."
            CHILD_EXIT_CODE=125  # Special exit code for leftover processes
        fi
        
        echo "[run_with_timeout] DEBUG: timeout path final exit code=$CHILD_EXIT_CODE"
        break
    fi
    
    # Short sleep before checking again
    sleep 1
done

ELAPSED_FINAL=$(($(date +%s) - START_TIME))

# Log final status BEFORE closing file descriptors
# This is the last message that will go through the tee pipe
echo "[run_with_timeout] Command finished with exit code $CHILD_EXIT_CODE after ${ELAPSED_FINAL}s"

# Save original stdout so we can still write human-reviewable output later
exec 3>&1

# CRITICAL: Close stdout/stderr to signal to the tee wrapper that we're done
# Any remaining output goes to a log file instead
exec 1>> /tmp/run_with_timeout_${CHILD_PID}.log 2>&1

echo "[run_with_timeout] Stdout/stderr redirected to /tmp/run_with_timeout_${CHILD_PID}.log"
echo "[run_with_timeout] DEBUG: entering post-redirect final cleanup block"
echo "[run_with_timeout] DEBUG: final cleanup params: STORED_Pgid=$STORED_Pgid, STORED_SID=$STORED_SID"

# Flush any remaining output
sync 2>/dev/null || true

# Small delay to ensure previous messages are flushed through tee
sleep 0.5

# Final orphan check (messages go to log file now)
echo "[run_with_timeout] DEBUG: final_cleanup calling cleanup_all_processes"
cleanup_all_processes "$STORED_Pgid" "$STORED_SID" "true"
echo "[run_with_timeout] DEBUG: final_cleanup after cleanup_all_processes: total_killed=$CLEANUP_KILLED_PROCESSES"
echo "[run_with_timeout] DEBUG: final_cleanup calling kill_known_problem_processes"
kill_known_problem_processes
echo "[run_with_timeout] DEBUG: final_cleanup after known_problem: total_killed=$CLEANUP_KILLED_PROCESSES"

echo "[run_with_timeout] Final cleanup done, exiting with code $CHILD_EXIT_CODE"

# Output full process list to original stdout for human review
echo "[run_with_timeout] Process list at exit:" >&3
if [[ $IS_WINDOWS -eq 1 ]]; then
    tasklist >&3 2>/dev/null || true
else
    ps aux >&3 2>/dev/null || true
fi

exit $CHILD_EXIT_CODE