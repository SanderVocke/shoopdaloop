# Plan: Migrate from ShoopQObject to cxx-qt's QObject

## Problem
After upgrading cxx-qt from 0.7.2 to 0.8.1, we renamed our QObject wrapper to `ShoopQObject` to avoid conflicts. However, this causes Qt signal-slot connection failures because:
- Qt's signals use `QObject*` in their MOC signature strings
- Our slots use `ShoopQObject*` in MOC signature strings
- Even though `using ShoopQObject = QObject;` makes them the same C++ type, MOC records the typedef name, not the underlying type

## Solution
Use cxx-qt's built-in `QObject` type directly, adding missing functionality as extension traits/helper functions.

## Phase 1: Core Infrastructure Changes (cxx_qt_lib_shoop)

### 1.1 Remove the typedef alias
- Delete `using ShoopQObject = QObject;` from `qobject.h`
- Keep helper functions but change parameter types from `ShoopQObject` to `QObject` (or `cxx_qt::QObject`)

### 1.2 Replace ShoopQObject type with cxx_qt::QObject in Rust bridge
- Change `type ShoopQObject;` to use cxx-qt's QObject
- The cxx-qt-lib QObject is accessed via `cxx_qt::QObject`

### 1.3 Add missing QObject functionality
Implement these as extension trait or free functions:

| Function | Implementation approach |
|----------|------------------------|
| `qobject_thread()` | Free function using QObject.thread() via C++ helper |
| `qobject_class_name()` | Free function, C++ helper accessing metaObject() |
| `qobject_meta_type_name()` | Free function, C++ helper |
| `qobject_property_*` | Free functions via C++ helpers (property system) |
| `qobject_set_property_*` | Free functions via C++ helpers |
| `qobject_find_child()` | Free function via C++ helper |
| `qobject_move_to_thread()` | Free function via C++ helper |
| `qobject_register_qml_singleton_instance()` | Free function via C++ helper |
| `qobject_has_signal/slot/property()` | Free functions via C++ helpers |
| `is_on_object_thread()` | Free function (replaces `amIOnObjectThread`) |

### 1.4 Update AsQObject/FromQObject traits
- Change to work with `cxx_qt::QObject` instead of `ShoopQObject`
- These provide the manual casting mechanism that complements cxx-qt's Upcast trait

## Phase 2: Frontend Migration (Incremental)

### 2.1 Pick a test QObject
- Select one that extensively uses QObject features
- Exclude other QObjects from build temporarily
- Make it work with new approach

### 2.2 Update bridge files
- Replace `type ShoopQObject = cxx_qt_lib_shoop::qobject::ShoopQObject;` with cxx-qt's QObject
- Update slot signatures to use `QObject*` (matching Qt signals)
- Implement `AsQObject` trait using `cxx_qt::Upcast<QObject>`

### 2.3 Update implementation files
- Replace `ShoopQObject` references with `cxx_qt::QObject`
- Update signal connection strings to use `QObject*`

### 2.4 Add back QObjects incrementally
- One by one, update each QObject bridge/implementation
- Fix any cross-dependencies

## Phase 3: Cleanup

### 3.1 Remove ShoopQObject typedef
- Delete all remaining ShoopQObject references
- Remove frontend's separate ShoopQObject C++ class if no longer needed

### 3.2 Remove unused code
- Clean up any orphaned ShoopQObject helper code

## Key Technical Details

### cxx-qt's QObject type
```rust
// In cxx-qt:
pub type QObject = ...; // Direct binding to Qt's QObject

// Usage in bridges:
unsafe extern "C++" {
    include!(<QtCore/QObject>);
    type QObject = cxx_qt::QObject;
}
```

### Upcast trait (auto-implemented by cxx-qt)
```rust
// For any #[qobject] type with #[base = SomeBase]:
unsafe impl Upcast<SomeBase> for MyType { ... }
unsafe impl Upcast<QObject> for MyType { ... } // transitive
```

### Downcast trait
```rust
// Can downcast from QObject to specific type:
let specific = qobject.downcast::<MyType>();
```

### Signal connection fix
Before: `"on_qml_object_created(ShoopQObject*,QUrl)"` - fails
After: `"on_qml_object_created(QObject*,QUrl)"` - matches Qt's signal

## Files to Modify

### Core (cxx_qt_lib_shoop)
1. `src/rust/cxx_qt_lib_shoop/src/include/cxx-qt-lib-shoop/qobject.h` - remove typedef, keep helpers
2. `src/rust/cxx_qt_lib_shoop/src/rust/qobject.rs` - replace ShoopQObject with QObject

### Frontend (many files, incrementally)
1. All `*_bridge.rs` files - update type definitions
2. All implementation files - update ShoopQObject references
3. `build.rs` - temporarily exclude most QObjects

## Testing Strategy
1. Build with single test QObject first
2. Verify it compiles
3. Run and check signal connections work
4. Add more QObjects one by one
5. Full build + runtime test