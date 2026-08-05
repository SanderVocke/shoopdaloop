#!/usr/bin/env bash
# Coverage wrapper for Rust.
#
# Runs the supplied command under Rust source-based coverage, then merges
# the raw profiles and emits an lcov report via llvm-cov. The wrapper is
# invoked from the CI test job for both Rust and QML coverage reports.

set -euo pipefail

if (( $# == 0 )); then
    echo "Usage: $0 <command> [args...]" >&2
    exit 2
fi

report_dir="${REPORTDIR:-${PWD}/coverage_reports}"
report_name="${LLVM_REPORTNAME:-report_llvm}"
work_dir="${WORKDIR:-${PWD}}"
llvm_cov="${LLVM_COV:-llvm-cov}"
llvm_profdata="${LLVM_PROFDATA:-llvm-profdata}"
profile_dir="${LLVM_PROFILE_DIR:-${work_dir}/coverage_profiles}"

cd "$work_dir"
mkdir -p "$report_dir" "$profile_dir"

# All four tools are required: llvm-cov and llvm-profdata are provided by the
# llvm-tools-preview Rust component; file and readelf are part of the standard
# build/container image.
for tool in file readelf; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "Required coverage tool not found: $tool" >&2
        exit 1
    fi
done
for tool in "$llvm_cov" "$llvm_profdata"; do
    if [[ ! -x "$tool" ]]; then
        echo "Coverage tool not found or not executable: $tool" >&2
        exit 1
    fi
done

# Reset previous state so each invocation produces a clean report.
rm -rf "$profile_dir" coverage.profdata
mkdir -p "$profile_dir"
export LLVM_PROFILE_FILE="$profile_dir/default_%m_%p.profraw"

echo "Using llvm-cov as: $llvm_cov"
echo "Using llvm-profdata as: $llvm_profdata"
echo "Report directory: $report_dir"
echo "Profile directory: $profile_dir"
echo "Running in: $(pwd)"

echo "---------------------------------------"
echo "Running command:"
echo "---------------------------------------"
printf ' %q' "$@"
printf '\n'

status=0
"$@" 2>&1 || status=$?

echo "---------------------------------------"
echo "Command exited with status: $status"
echo "---------------------------------------"

if (( status != 0 )); then
    echo "ERROR: test command exited with code $status" >&2
    exit "$status"
fi

# Collect every .profraw file emitted during the run.
declare -a profraw_files=()
while IFS= read -r -d '' f; do
    profraw_files+=("$f")
done < <(find "$profile_dir" -type f -name '*.profraw' -print0)

profraw_count="${#profraw_files[@]}"
echo "Found ${profraw_count} .profraw files"
if (( profraw_count == 0 )); then
    echo "ERROR: no .profraw files were generated" >&2
    exit 1
fi

echo "---------------------------------------"
echo "Merging LLVM profdata"
echo "---------------------------------------"
"$llvm_profdata" merge \
    -sparse \
    "${profraw_files[@]}" \
    -o coverage.profdata

# Collect every ELF object that actually carries coverage instrumentation.
# The portable folder and the archived nextest test binaries both qualify;
# Qt shared libraries, the cargo-nextest binary, and any other uninstrumented
# ELF are filtered out so llvm-cov does not reject them.
declare -a objects=()
while IFS= read -r -d '' candidate; do
    if ! file -b "$candidate" | grep -q '^ELF '; then
        continue
    fi
    if ! readelf -SW "$candidate" 2>/dev/null | grep -q '__llvm_covmap'; then
        continue
    fi
    objects+=("$candidate")
done < <(find "$work_dir" -type f \( -name '*.so' -o -executable \) -print0)

object_count="${#objects[@]}"
echo "Found ${object_count} instrumented ELF objects"
if (( object_count == 0 )); then
    echo "ERROR: no instrumented ELF objects containing __llvm_covmap were found under $work_dir" >&2
    exit 1
fi

declare -a object_args=()
for object in "${objects[@]}"; do
    object_args+=("-object" "$object")
done

echo "---------------------------------------"
echo "Generating LLVM lcov report"
echo "---------------------------------------"
report_path="$report_dir/$report_name.info"
export_status=0
"$llvm_cov" export \
    -instr-profile=coverage.profdata \
    -format=lcov \
    "${object_args[@]}" \
    > "$report_path" || export_status=$?

# llvm-cov can return non-zero after successfully exporting the available data when this
# multi-object set includes instrumented binaries that the current one-process profile did not
# execute. Accept only a structurally complete, non-empty LCOV report in that case; all empty or
# malformed output remains fatal.
if [[ ! -s "$report_path" ]] || ! grep -q '^SF:' "$report_path" || ! grep -q '^end_of_record$' "$report_path"; then
    echo "ERROR: generated report $report_path is empty or malformed (llvm-cov status $export_status)" >&2
    exit 1
fi
if (( export_status != 0 )); then
    echo "WARNING: llvm-cov returned $export_status after producing a complete report; continuing" >&2
fi
