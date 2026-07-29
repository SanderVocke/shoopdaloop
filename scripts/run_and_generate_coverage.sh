#!/bin/bash
# Coverage script for Rust (via llvm-cov / profraw).

CMD="$@"
_REPORTDIR=${REPORTDIR:-${PWD}/coverage_reports}
_LLVM_REPORTNAME=${LLVM_REPORTNAME:-report_llvm}
_WORKDIR=${WORKDIR:-${PWD}}
_LLVM_COV=${LLVM_COV:-llvm-cov}
_LLVM_PROFDATA=${LLVM_PROFDATA:-llvm-profdata}

cd ${_WORKDIR}

echo "Using llvm-cov as: ${_LLVM_COV}"
echo "Using llvm-profdata as: ${_LLVM_PROFDATA}"
echo "Running in: $(pwd)"

mkdir -p ${_REPORTDIR}

# Clean old profraw
c="find . -name '*.profraw' -delete; rm -f *.profdata"
echo "---------------------------------------"
echo "Cleaning: ${c}"
echo "---------------------------------------"
bash -c "${c}"

# Run command
echo "---------------------------------------"
echo "Running command: ${CMD}"
echo "---------------------------------------"
${CMD} 2>&1
RESULT=$?
if [ ! $RESULT -eq 0 ]; then
    echo "ERROR: exited with code $RESULT" >&2
    exit $RESULT
fi

# Count profraw files
profraw_count=`find . -name '*.profraw' | wc -l`
echo "---------------------------------------"
echo "Found ${profraw_count} .profraw files"
echo "---------------------------------------"

# Merge profraw files
PROFRAW_FILES=$(find . -name '*.profraw')
if [ -z "${PROFRAW_FILES}" ]; then
    echo "WARNING: No .profraw files found - skipping LLVM coverage merge"
else
    c="${_LLVM_PROFDATA} merge -sparse ${PROFRAW_FILES} -o coverage.profdata"
    echo "---------------------------------------"
    echo "Merging LLVM profdata: ${c}"
    echo "---------------------------------------"
    ${c}
fi

# Generate lcov-format report from profdata
c="find . -type f \( -name '*.so' -o -executable \) -print0 | xargs -0 file | grep 'ELF' | cut -d: -f1 | tr '\n' '\0' | xargs -0 ${_LLVM_COV} export -instr-profile=coverage.profdata -format=lcov > ${_REPORTDIR}/${_LLVM_REPORTNAME}.info"
echo "---------------------------------------"
echo "Generating LLVM lcov report: ${c}"
echo "---------------------------------------"
bash -c "${c}"
