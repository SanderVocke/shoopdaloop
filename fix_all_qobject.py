#!/usr/bin/env python3
import os
import re

def fix_file(filepath):
    with open(filepath, 'r') as f:
        content = f.read()
    
    # Skip if already has QObject declared via QtCore
    if 'include!(<QtCore/QObject>)' in content:
        print(f"Skip (has QObject): {filepath}")
        return True
    
    # Check if file uses QObject
    if 'QObject' not in content:
        print(f"Skip (no QObject use): {filepath}")
        return True
    
    # Multiple patterns to try:
    patterns = [
        # Pattern 1: First extern "C++" block with content
        (r'(pub mod ffi \{\n    unsafe extern "C++" \{\n        include!)', 
         r'pub mod ffi {\n    unsafe extern "C++" {\n        include!(<QtCore/QObject>);\n        type QObject;\n\n        include!'),
        
        # Pattern 2: Empty-ish block after earlier corruption
        (r'(pub mod ffi \{\n    unsafe extern "C++" \{\n        include!\("cxx-qt-lib/qvariant.h"\);)',
         r'pub mod ffi {\n    unsafe extern "C++" {\n        include!(<QtCore/QObject>);\n        type QObject;\n\n        include!("cxx-qt-lib/qvariant.h");'),
        
        # Pattern 3: After bridge declaration directly
        (r'(#\[cxx_qt::bridge\]\npub mod ffi \{\n    unsafe extern "C++" \{\n)',
         r'#[cxx_qt::bridge]\npub mod ffi {\n    unsafe extern "C++" {\n        include!(<QtCore/QObject>);\n        type QObject;\n\n'),
    ]
    
    modified = False
    for pattern, replacement in patterns:
        if re.search(pattern, content):
            content = re.sub(pattern, replacement, content, count=1)
            modified = True
            break
    
    if modified:
        with open(filepath, 'w') as f:
            f.write(content)
        print(f"Fixed: {filepath}")
        return True
    
    # If none matched, try a more generic approach
    # Find first occurrence of unsafe extern "C++" { and add QObject after it
    match = re.search(r'(pub mod ffi \{\s*unsafe extern "C++" \{)', content)
    if match:
        # Insert after the opening brace
        insert_pos = match.end()
        new_content = content[:insert_pos] + '\n        include!(<QtCore/QObject>);\n        type QObject;\n' + content[insert_pos:]
        with open(filepath, 'w') as f:
            f.write(new_content)
        print(f"Fixed (generic): {filepath}")
        return True
    
    print(f"Could not fix: {filepath}")
    return False

# Process all bridge files
bridge_files = []
for root, dirs, files in os.walk('src/rust/frontend/src/cxx_qt_shoop/rust'):
    for f in files:
        if f.endswith('_bridge.rs'):
            bridge_files.append(os.path.join(root, f))

success_count = 0
for filepath in bridge_files:
    if fix_file(filepath):
        success_count += 1

print(f"\nSuccessfully handled {success_count}/{len(bridge_files)} bridge files")
