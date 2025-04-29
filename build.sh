#!/bin/bash

if ! command -v uv &> /dev/null; then
    echo "Error: 'uv' not found in PATH. Please install it first (see https://github.com/astral-sh/uv)"
    exit 1
fi

uv run scripts/build.py "$@"
