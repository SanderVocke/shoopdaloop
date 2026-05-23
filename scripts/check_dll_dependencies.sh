#!/usr/bin/env bash
# check_dll_dependencies.sh - Validates that all DLL dependencies for an executable can be resolved
#
# Usage: check_dll_dependencies.sh <executable_path>
#
# This script uses Dependencies.exe CLI (lucasg/Dependencies) to analyze PE dependencies
# and check if all required DLLs can be resolved given the current PATH and library paths.
#
# This is crucial for CI because Windows DLL load failures are silent - the executable
# exits immediately with no output, making debugging extremely difficult.
#
# Prerequisites:
# - Dependencies.exe must be in PATH (downloaded in CI workflow)
# - Visual C++ Redistributable must be installed (standard on GitHub runners)

set -euo pipefail

EXE_PATH="$1"

if [[ -z "$EXE_PATH" ]]; then
    echo "Usage: $0 <executable_path>" >&2
    exit 1
fi

if [[ ! -f "$EXE_PATH" ]]; then
    echo "::error::Executable not found: $EXE_PATH" >&2
    exit 1
fi

EXE_NAME=$(basename "$EXE_PATH")

echo ""
echo "=========================================="
echo "DLL Dependency Check: $EXE_NAME"
echo "=========================================="
echo "Executable: $EXE_PATH"
echo ""
echo "Library search paths (PATH):"
echo "$PATH" | tr ':' '\n' | while read -r p; do
    if [[ -d "$p" ]]; then
        echo "  - $p (exists)"
    else
        echo "  - $p (not found)"
    fi
done | head -20
echo ""

# Check if Dependencies.exe is available
if ! command -v Dependencies.exe &>/dev/null; then
    echo "::warning::Dependencies.exe not found in PATH, skipping DLL dependency validation"
    echo "To enable this check, download Dependencies from:"
    echo "https://github.com/lucasg/Dependencies/releases"
    exit 0
fi

echo "Running Dependencies CLI analysis..."
echo ""

# Dependencies CLI options:
# -imports: show all imports (what DLLs are needed)
# -modules: show module resolution status (whether DLLs are found)
#
# We run both to get complete picture:
# 1. First list imports to see what's needed
# 2. Then check modules to see resolution status
#
# NOTE: Windows API Set DLLs (ext-ms-*, api-ms-*) appear as NOT_FOUND
# because they're virtual DLLs resolved by the OS loader, not real files.
# See: https://learn.microsoft.com/en-us/windows/win32/apiindex/api-set-loading

# Capture output for analysis
IMPORTS_OUTPUT=""
MODULES_OUTPUT=""
DEPS_EXIT_CODE=0

# Run imports check (what DLLs does this exe need?)
echo "--- Imports (required DLLs) ---"
IMPORTS_OUTPUT=$(Dependencies.exe -imports "$EXE_PATH" 2>&1) || DEPS_EXIT_CODE=$?
echo "$IMPORTS_OUTPUT" | head -50
echo ""

# Run modules check (can they be resolved?)
echo "--- Module Resolution Status ---"
MODULES_OUTPUT=$(Dependencies.exe -modules "$EXE_PATH" 2>&1) || DEPS_EXIT_CODE=$?
echo "$MODULES_OUTPUT"
echo ""

# Note about Windows API Set DLLs
NOT_FOUND_API_SETS=$(echo "$MODULES_OUTPUT" | grep "\[NOT_FOUND\]" | grep -cE "ext-ms-|api-ms-" || true)
if [[ "$NOT_FOUND_API_SETS" -gt 0 ]]; then
    echo "Note: $NOT_FOUND_API_SETS [NOT_FOUND] entries are for Windows API Set DLLs (ext-ms-*, api-ms-*)"
    echo "These are VIRTUAL DLLs resolved by the OS loader - this is NORMAL and expected."
    echo ""
fi

# Analyze for unresolved/missing dependencies
# Common patterns in Dependencies output for missing DLLs:
# - "not found"
# - "unresolved"
# - "missing"
# - Module listed without a resolved path
# - Error messages about specific DLLs

# IMPORTANT: Windows API Set DLLs (ext-ms-*, api-ms-*) are VIRTUAL DLLs
# that don't exist as files. They're resolved by the Windows loader at runtime.
# We should ignore these as they're not actual missing dependencies.
# See: https://learn.microsoft.com/en-us/windows/win32/apiindex/api-set-loading
API_SET_PATTERN="^ext-ms-|^api-ms-"

HAS_ISSUES=false

# Check imports output for issues (but filter out Windows API Set DLLs)
IMPORTS_ISSUES=$(echo "$IMPORTS_OUTPUT" | grep -iE "not found|unresolved|missing|error|failed" | grep -viE "$API_SET_PATTERN" || true)
if [[ -n "$IMPORTS_ISSUES" ]]; then
    HAS_ISSUES=true
    echo "::warning::Potential issues detected in imports analysis"
fi

# Check modules output for issues (but filter out Windows API Set DLLs)
# The [ApiSetSchema] entries are RESOLVED API Sets - those are fine
MODULES_ISSUES=$(echo "$MODULES_OUTPUT" | grep -iE "\[NOT_FOUND\]" | grep -viE "$API_SET_PATTERN" || true)
if [[ -n "$MODULES_ISSUES" ]]; then
    HAS_ISSUES=true
fi

# Also check for any unresolved entries that aren't API Sets
UNRESOLVED=$(echo "$MODULES_OUTPUT" | grep -iE "unresolved" | grep -viE "$API_SET_PATTERN" || true)
if [[ -n "$UNRESOLVED" ]]; then
    HAS_ISSUES=true
fi

# Also try to actually run the executable briefly to catch any dynamic load issues
# that Dependencies might miss (LoadLibrary, etc.)
echo "--- Quick Launch Test ---"
echo "Attempting brief launch to catch dynamic DLL load errors..."

# Create a small timeout - we just want to see if it can start
# Use --help or similar non-destructive flag if available
LAUNCH_OUTPUT=""
LAUNCH_EXIT=0

# Try running with a simple command that should exit quickly
# Most test binaries accept --help or --list-test-names-only
LAUNCH_OUTPUT=$(timeout 5 "$EXE_PATH" --help 2>&1) || LAUNCH_EXIT=$?
if [[ $LAUNCH_EXIT -eq 124 ]]; then
    echo "Launch timed out (expected for some binaries)"
elif [[ $LAUNCH_EXIT -ne 0 ]]; then
    echo "::warning::Quick launch failed with exit code $LAUNCH_EXIT"
    echo "Launch output:"
    echo "$LAUNCH_OUTPUT"
    HAS_ISSUES=true
else
    echo "Quick launch succeeded (exit code 0)"
fi
echo ""

# Final report
echo "=========================================="
echo "Dependency Check Summary"
echo "=========================================="

if [[ "$HAS_ISSUES" == "true" ]]; then
    echo ""
    echo "::error::DLL DEPENDENCY CHECK FAILED for $EXE_NAME"
    echo ""
    echo "One or more DLL dependencies could not be resolved."
    echo ""
    echo "Missing/unresolved DLL details:"
    echo ""

    # Extract specific missing DLL lines for clearer error (excluding API Set DLLs)
    {
        echo "$IMPORTS_ISSUES"
        echo "$MODULES_ISSUES"
        echo "$UNRESOLVED"
    } | grep -v "^$" | sort -u | head -30

    echo ""
    echo "Suggested fixes:"
    echo "1. Check that all required DLLs are in portable/lib/"
    echo "2. Verify PATH includes the directory containing the missing DLL"
    echo "3. Check for Debug vs Release DLL mismatches (e.g., VCRUNTIME140D.dll vs VCRUNTIME140.dll)"
    echo "4. For vcpkg dependencies, ensure the correct triplet DLLs are included"

    exit 1
fi

echo ""
echo "✓ All DLL dependencies resolved for $EXE_NAME"
echo "The executable should be able to start successfully."
echo ""
echo "Note: Windows API Set DLLs (ext-ms-*, api-ms-*) are resolved by the OS"
echo "and are not required to be present as files."
exit 0