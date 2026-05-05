#!/usr/bin/env python3
import os
import re

def fix_file(filepath):
    with open(filepath, 'r') as f:
        content = f.read()
    
    # Check if already has QObject declared via QtCore
    if 'include!(<QtCore/QObject>)' in content:
        print(f"Skip (has QObject): {filepath}")
        return
    
    # Check if file uses QObject
    if 'QObject' not in content:
        print(f"Skip (no QObject use): {filepath}")
        return
    
    # Pattern to match first extern "C++" block start
    # We want to add QObject declaration after the opening brace of the first block
    
    # Find the first extern "C++" block in the ffi module
    pattern = r'(pub mod ffi \{\s*unsafe extern "C++" \{\s*)'
    
    if re.search(pattern, content):
        # Add QObject declaration
        replacement = r'pub mod ffi {\n    unsafe extern "C++" {\n        include!(<QtCore/QObject>);\n        type QObject;\n\n'
        content = re.sub(pattern, replacement, content)
        
        with open(filepath, 'w') as f:
            f.write(content)
        print(f"Fixed: {filepath}")
    else:
        print(f"Pattern not found: {filepath}")

# Process all bridge files
bridge_files = []
for root, dirs, files in os.walk('src/rust/frontend/src/cxx_qt_shoop/rust'):
    for f in files:
        if f.endswith('_bridge.rs'):
            bridge_files.append(os.path.join(root, f))

for filepath in bridge_files:
    fix_file(filepath)

print(f"\nProcessed {len(bridge_files)} bridge files")
