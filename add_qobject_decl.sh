#!/bin/bash
# Script to add QObject type declaration to extern "C++" blocks

set -e

cd /home/sander/dev/shoopdaloop-4

# List of bridge files that need QObject in extern "C++" blocks
FILES=$(grep -l '\*mut QObject' src/rust/frontend/src/cxx_qt_shoop/rust/*_bridge.rs src/rust/frontend/src/cxx_qt_shoop/rust/test/*_bridge.rs 2>/dev/null)

for file in $FILES; do
    # Check if the file already has QObject type declared (not via cxx_qt_lib_shoop)
    if grep -q "include!(<QtCore/QObject>);" "$file"; then
        echo "Already has QObject declaration: $file"
        continue
    fi
    
    # Check if file has extern "C++" blocks after extern "RustQt" that use QObject
    # Look for the pattern where we have a second extern "C++" block with QObject usage
    
    # Simple approach: add QObject declaration to each extern "C++" block that's after extern "RustQt"
    # We'll use a more targeted approach: find lines with QObject in extern "C++" context
    
    echo "Processing: $file"
    
    # Find all extern "C++" block start lines
    # For each block, check if it uses QObject and doesn't already have it declared
    
    # Use perl for more precise editing
    perl -i -pe '
        # When we see unsafe extern "C++" block start
        if (/unsafe extern "C++" \{/ && !/include!\(<QtCore\/QObject>\);/) {
            # Mark position to potentially add QObject declaration
            $need_qobject = 1;
        }
        
        # If we see QObject used in this block (but not the declaration line itself)
        if (/\*mut QObject/ && !/type QObject;/ && !/include!\(<QtCore\/QObject>\);/ && $need_qobject) {
            # This block needs QObject - add it at the start of the block
            # Actually we should add it before the first include in the block
            $add_qobject = 1;
        }
    ' "$file"
    
    echo "Done: $file"
done

echo "All files processed!"