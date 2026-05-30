#!/bin/bash
# Script to migrate all bridge files from ShoopQObject to QObject

set -e

cd /home/sander/dev/shoopdaloop-4

# Find all bridge files that still contain ShoopQObject
FILES=$(grep -l "ShoopQObject" src/rust/frontend/src/cxx_qt_shoop/rust/*_bridge.rs)

for file in $FILES; do
    echo "Processing: $file"
    
    # 1. Remove duplicate include lines for qobject.h
    sed -i '/include!("cxx-qt-lib-shoop\/qobject.h");/{
        N
        /\ninclude!("cxx-qt-lib-shoop\/qobject.h");/D
    }' "$file"
    
    # 2. Remove the single include line for qobject.h (after duplicates removed)
    sed -i '/include!("cxx-qt-lib-shoop\/qobject.h");/d' "$file"
    
    # 3. Remove the ShoopQObject type alias line
    sed -i '/type ShoopQObject = cxx_qt_lib_shoop::qobject::ShoopQObject;/d' "$file"
    
    # 4. Replace *mut ShoopQObject with *mut QObject in function signatures
    sed -i 's/\*mut ShoopQObject/\*mut QObject/g' "$file"
    
    # 5. Replace &ShoopQObject with &QObject
    sed -i 's/&ShoopQObject/\&QObject/g' "$file"
    
    # 6. Replace &mut ShoopQObject with &mut QObject
    sed -i 's/&mut ShoopQObject/\&mut QObject/g' "$file"
    
    # 7. Replace Pin<&mut ShoopQObject> with Pin<&mut QObject>
    sed -i 's/Pin<&mut ShoopQObject>/Pin<\&mut QObject>/g' "$file"
    
    # 8. Replace ShoopQObject in AsQObject implementations
    sed -i 's/\*mut ffi::ShoopQObject/\*mut ffi::QObject/g' "$file"
    sed -i 's/\*const ffi::ShoopQObject/\*const ffi::QObject/g' "$file"
    
    # 9. Replace in FromQObject implementations
    sed -i 's/cxx_qt_lib_shoop::qobject::ShoopQObject/cxx_qt::QObject/g' "$file"
    
    # 10. Replace in qobjectFromPtr/qobjectFromRef return types
    sed -i 's/\*mut ShoopQObject/\*mut QObject/g' "$file"
    sed -i 's/\&ShoopQObject/\&QObject/g' "$file"
    
    # 11. Handle special case: fromQObjectRef/fromQObjectMut that use ShoopQObject as parameter
    # These should already be caught by previous replacements
    
    echo "Done: $file"
done

echo "All bridge files processed!"