#!/bin/bash

CMD="$@"
_BASEDIR=${BASEDIR}
_EXCLUDE=${EXCLUDE}
_BUILDDIR=${BUILDDIR}
_REPORTDIR=${REPORTDIR}
_LCOV=${LCOV:-lcov}
_GCOV=${GCOV:-gcov}
_LCOV_ARGS=${LCOV_ARGS:-"--ignore-errors source,negative"}
_GENHTML=${GENHTML:-genhtml}
_REPORTNAME=${REPORTNAME:-report}

_LCOV="${_LCOV} --gcov-tool ${_GCOV} -b ${_BASEDIR} -d ${_BUILDDIR} ${_LCOV_ARGS}"
echo "Using lcov as: ${_LCOV}"

mkdir -p ${_REPORTDIR}

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
${CMD}

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

# HTML
c="${_GENHTML} -o ${_REPORTDIR}/${_REPORTNAME} ${_REPORTDIR}/${_REPORTNAME}.info"
echo "---------------------------------------"
echo "Generating HTML: ${c}"
echo "---------------------------------------"
${c}


