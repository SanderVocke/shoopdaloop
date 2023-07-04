#!/bin/bash

PROJECT_DIR=${SCRIPT_DIR}/../../..

# template items
VERSION=$($PROJECT_DIR/scripts/parse_pyproject_toml.sh $PROJECT_DIR/pyproject.toml version)
DESCRIPTION=$($PROJECT_DIR/scripts/parse_pyproject_toml.sh $PROJECT_DIR/pyproject.toml description)
DEPENDS=$(cat ${SCRIPT_DIR}/../../dependencies/arch_run_abstract*.txt | tr '\n' ' ')
MAKEDEPENDS=$(cat ${SCRIPT_DIR}/../../dependencies/arch_build*.txt | tr '\n' ' ')
CHECKDEPENDS=$(cat ${SCRIPT_DIR}/../../dependencies/arch_check*.txt | tr '\n' ' ')