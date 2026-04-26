#!/bin/bash

CMD="$@"
_BASEDIR=${BASEDIR}
_EXCLUDE=${EXCLUDE}
_BUILDDIR=${BUILDDIR}
_REPORTDIR=${REPORTDIR:-${PWD}/coverage_reports}
_LCOV=${LCOV:-lcov}
_GCOV=${GCOV:-gcov}
_LCOV_ARGS=${LCOV_ARGS}
_GENHTML=${GENHTML:-genhtml}
_LCOV_REPORTNAME=${LCOV_REPORTNAME:-report_lcov}
_LLVM_REPORTNAME=${LLVM_REPORTNAME:-report_llvm}
_ORI_BUILD_DIR=${ORI_BUILD_DIR}
_DO_GENHTML=${DO_GENHTML:-1}
_WORKDIR=${WORKDIR:-${PWD}}
_LLVM_COV=${LLVM_COV:-llvm-cov}
_LLVM_PROFDATA=${LLVM_PROFDATA:-llvm-profdata}

cd ${_WORKDIR}

_LCOV="${_LCOV} --ignore-errors source --ignore-errors gcov --gcov-tool ${_GCOV} -b ${_BASEDIR} -d ${_BUILDDIR} ${_LCOV_ARGS}"
echo "Using lcov as: ${_LCOV}"
echo "Using llvm-cov as: ${_LLVM_COV}"
echo "Using llvm-profdata as: ${_LLVM_PROFDATA}"
echo "Running in: $(pwd)"

mkdir -p ${_REPORTDIR}

if [ ! -z ${_ORI_BUILD_DIR} ]; then
    export GCOV_PREFIX=${_BUILDDIR}
    export GCOV_PREFIX_STRIP=$(echo ${_ORI_BUILD_DIR} | tr '/' ' ' | wc -w)
    echo "Remapping build dir: ${_ORI_BUILD_DIR} -> ${_BUILDDIR}"
    echo "  - GCOV_PREFIX = ${GCOV_PREFIX}"
    echo "  - GCOV_PREFIX_STRIP = ${GCOV_PREFIX_STRIP}"
fi

# Clean
c="${_LCOV} -z ; find . -name '*.profraw' -delete; rm *.profdata | true"
echo "---------------------------------------"
echo "Cleaning: ${c}"
echo "---------------------------------------"
bash -c "${c}"

# Generate baseline
if [ ! -f ${_REPORTDIR}/${_LCOV_REPORTNAME}.info ]; then
    c="${_LCOV} -c -i -o ${_REPORTDIR}/${_LCOV_REPORTNAME}.base"
    echo "---------------------------------------"
    echo "Generating baseline: ${c}"
    echo "---------------------------------------"
    ${c}
fi

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

# Count GCDA files
gcda_count=`find . -name "*.gcda" | wc -l`
echo "---------------------------------------"
echo "Found ${gcda_count} gcda files"
echo "---------------------------------------"

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

# Profraw reporting
c="${_LLVM_COV} export -instr-profile=coverage.profdata -format=lcov > ${_REPORTDIR}/${_LLVM_REPORTNAME}.info"
echo "---------------------------------------"
echo "Generating LLVM lcov report: ${c}"
echo "---------------------------------------"
${c}

# Capture
c="${_LCOV} -c -o ${_REPORTDIR}/${_LCOV_REPORTNAME}.capture"
echo "---------------------------------------"
echo "Generating lcov report: ${c}"
echo "---------------------------------------"
${c}

# Add baseline
c="${_LCOV} -a ${_REPORTDIR}/${_LCOV_REPORTNAME}.base -a ${_REPORTDIR}/${_LCOV_REPORTNAME}.capture -o ${_REPORTDIR}/${_LCOV_REPORTNAME}.total"
echo "---------------------------------------"
echo "Appending lcov baseline: ${c}"
echo "---------------------------------------"
${c}

# Filter
c="${_LCOV} -r ${_REPORTDIR}/${_LCOV_REPORTNAME}.total ${_EXCLUDE} -o ${_REPORTDIR}/${_LCOV_REPORTNAME}.info"
echo "---------------------------------------"
echo "Filtering lcov report: ${c}"
echo "---------------------------------------"
${c}


if [ $_DO_GENHTML -ne 0 ]; then
    # HTML
    c="${_GENHTML} -o ${_REPORTDIR}/${_LCOV_REPORTNAME} ${_REPORTDIR}/${_LCOV_REPORTNAME}.info"
    echo "---------------------------------------"
    echo "Generating lcov HTML: ${c}"
    echo "---------------------------------------"
    ${c}

    # LLVM HTML
    # Profraw reporting
    c="mkdir -p ${_REPORTDIR}/${_LLVM_REPORTNAME}_html; ${_LLVM_COV} show -instr-profile=coverage.profdata -format=html -output-dir=${_REPORTDIR}/${_LLVM_REPORTNAME}_html"
    echo "---------------------------------------"
    echo "Generating LLVM lcov HTML: ${c}"
    echo "---------------------------------------"
    ${c}
else
    echo "---------------------------------------"
    echo "Skipping HTML generation"
    echo "---------------------------------------"
fi


