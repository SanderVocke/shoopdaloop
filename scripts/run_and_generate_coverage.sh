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
_REPORTNAME=${REPORTNAME:-report}
_ORI_BUILD_DIR=${ORI_BUILD_DIR}
_DO_GENHTML=${DO_GENHTML:-1}
_WORKDIR=${WORKDIR:-${PWD}}

cd ${_WORKDIR}

_LCOV="${_LCOV} --ignore-errors source --gcov-tool ${_GCOV} -b ${_BASEDIR} -d ${_BUILDDIR} ${_LCOV_ARGS}"
echo "Using lcov as: ${_LCOV}"
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
c="${_LCOV} -z"
echo "---------------------------------------"
echo "Cleaning: ${c}"
echo "---------------------------------------"
${c}

# Generate baseline
if [ ! -f ${_REPORTDIR}/${_REPORTNAME}.info ]; then
    c="${_LCOV} -c -i -o ${_REPORTDIR}/${_REPORTNAME}.base"
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

# Capture
c="${_LCOV} -c -o ${_REPORTDIR}/${_REPORTNAME}.capture"
echo "---------------------------------------"
echo "Generating report: ${c}"
echo "---------------------------------------"
${c}

# Add baseline
c="${_LCOV} -a ${_REPORTDIR}/${_REPORTNAME}.base -a ${_REPORTDIR}/${_REPORTNAME}.capture -o ${_REPORTDIR}/${_REPORTNAME}.total"
echo "---------------------------------------"
echo "Appending baseline: ${c}"
echo "---------------------------------------"
${c}

# Filter
c="${_LCOV} -r ${_REPORTDIR}/${_REPORTNAME}.total ${_EXCLUDE} -o ${_REPORTDIR}/${_REPORTNAME}.info"
echo "---------------------------------------"
echo "Filtering report: ${c}"
echo "---------------------------------------"
${c}


if [ $_DO_GENHTML -ne 0 ]; then
    # HTML
    c="${_GENHTML} -o ${_REPORTDIR}/${_REPORTNAME} ${_REPORTDIR}/${_REPORTNAME}.info"
    echo "---------------------------------------"
    echo "Generating HTML: ${c}"
    echo "---------------------------------------"
    ${c}
else
    echo "---------------------------------------"
    echo "Skipping HTML generation"
    echo "---------------------------------------"
fi


