#!/bin/bash
MASK_CMD=true

if [ "$failure_status" != "" ]; then
  if [[ -n "${GHA_RUN_ALWAYS}" ]]; then
    echo "failure_status is set, but GHA_RUN_ALWAYS is set. Running command."
    MASK_CMD=false # ensure that the end result fails
  else
    echo "failure_status is set"
    exit 1
  fi
fi

echo "GHA wrapper: running command: $BASE_WRAP_SHELL $@"

$BASE_WRAP_SHELL "$@" 2>&1 | tee -a log_all.txt
CMD_RESULT=${PIPESTATUS[0]}

# Process the result code
$(sh -c "exit ${CMD_RESULT}") && $(${MASK_CMD})
RESULT=$?
echo "Command result ${CMD_RESULT}, masked with ${MASK_CMD}, final result ${RESULT}"

# check if command was successful
if [ ${RESULT} -ne 0 ]; then
  echo "Command failed with exit code $?, setting status to failed"

  # Check if pi coding agent checkpoint is enabled (mutually exclusive with manual checkpoints)
  if [ "$pi_checkpoint_enabled" == "true" ]; then
    echo "Pi coding agent checkpoint enabled. Invoking pi coding agent..."

    # Create coding_agent directory and copy the build log there for context
    mkdir -p coding_agent
    cp log_all.txt coding_agent/

    # Set up pi config directory
    PI_CONFIG_DIR="$HOME/.pi/agent"
    mkdir -p "$PI_CONFIG_DIR"

    # Copy models.json if it exists in the repo
    if [ -f ".github/pi_coding_agent/models.json" ]; then
      cp ".github/pi_coding_agent/models.json" "$PI_CONFIG_DIR/models.json"
    fi

    # Copy auth.json if it exists in the repo
    if [ -f ".github/pi_coding_agent/auth.json" ]; then
      cp ".github/pi_coding_agent/auth.json" "$PI_CONFIG_DIR/auth.json"
    fi

    # Determine model to use
    PI_MODEL="${PI_MODEL:-openrouter/openai/gpt-oss-20b:free}"
    PI_TIMEOUT="${PI_TIMEOUT:-600}"  # 10 minutes default

    # Run pi coding agent
    echo "Running pi coding agent with model: $PI_MODEL"

    # Execute pi and tee output to log file
    export PI_MODEL
    export OPENCODE_API_KEY
    export OPENROUTER_API_KEY
    pi --stream=all --no-session --model "$PI_MODEL" -p "Read .github/pi_coding_agent/prompt.md and complete the task(s) inside." 2>&1 | tee "coding_agent/pi_output.log"

    PI_EXIT_CODE=${PIPESTATUS[0]}

    if [ $PI_EXIT_CODE -eq 124 ]; then
      echo "Pi coding agent timed out after ${PI_TIMEOUT}s"
    elif [ $PI_EXIT_CODE -ne 0 ]; then
      echo "Pi coding agent exited with code $PI_EXIT_CODE"
    else
      echo "Pi coding agent completed successfully"
    fi

    # Mark failure as handled so subsequent steps don't re-trigger
    echo "failure_status=handled" | tee -a $GITHUB_ENV
  else
    # Manual checkpoint mode (existing behavior)
    echo "failure_status=unhandled" | tee -a $GITHUB_ENV
    echo "ACTION_TMATE_CONNECT_TIMEOUT_SECONDS=3600" | tee -a $GITHUB_ENV
  fi
fi

exit $RESULT
