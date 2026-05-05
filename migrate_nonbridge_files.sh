#!/bin/bash
# Script to migrate non-bridge QObject implementation files from ShoopQObject to QObject

set -e

cd /home/sander/dev/shoopdaloop-4

# Find all non-bridge files that contain ShoopQObject
FILES=$(grep -l "ShoopQObject" src/rust/frontend/src/cxx_qt_shoop/rust/*.rs | grep -v "_bridge.rs")

for file in $FILES; do
    echo "Processing: $file"
    
    # Replace ShoopQObject with QObject in all contexts
    sed -i 's/ShoopQObject/QObject/g' "$file"
    
    echo "Done: $file"
done

# Also check other frontend source files
OTHER_FILES=$(grep -l "ShoopQObject" src/rust/frontend/src/*.rs src/rust/frontend/src/**/*.rs 2>/dev/null | grep -v "cxx_qt_shoop/rust" || true)

if [ -n "$OTHER_FILES" ]; then
    for file in $OTHER_FILES; do
        echo "Processing: $file"
        sed -i 's/ShoopQObject/QObject/g' "$file"
        echo "Done: $file"
    done
fi

echo "All non-bridge files processed!"