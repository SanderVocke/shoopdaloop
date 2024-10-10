#!/bin/sh

SCRIPT_DIR="$(dirname "$(realpath "$0")")"

PYSIDE6_LIBS=$(find $SCRIPT_DIR -wholename "*/site-packages/PySide6/Qt/lib")
export LD_LIBRARY_PATH="${PYSIDE6_LIBS}:${LD_LIBRARY_PATH}"
export LD_LIBRARY_PATH="${SCRIPT_DIR}/shoop_lib/py/lib:${LD_LIBRARY_PATH}"
export LD_LIBRARY_PATH="${SCRIPT_DIR}/shoop_lib:${LD_LIBRARY_PATH}"
export LD_LIBRARY_PATH="${SCRIPT_DIR}/lib:${LD_LIBRARY_PATH}"