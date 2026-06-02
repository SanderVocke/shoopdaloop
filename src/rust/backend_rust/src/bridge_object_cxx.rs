#![allow(dead_code)]

use std::any::Any;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};

#[cxx::bridge(namespace = "backend_rust")]
pub mod ffi {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    struct BridgeStrongHandle {
        id: u64,
        type_id: u32,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    struct BridgeWeakHandle {
        id: u64,
        type_id: u32,
    }

    unsafe extern "C++" {
        include!("internal/BridgeObject.h");

        fn bridge_upgrade_for_rust(weak_id: u64, weak_type_id: u32) -> bool;
        fn bridge_release_strong_for_rust(strong_id: u64, strong_type_id: u32);
    }

    extern "Rust" {
        fn bridge_is_rust_owned(id: u64) -> bool;
        fn bridge_rust_upgrade(weak_id: u64, weak_type_id: u32) -> bool;
        fn bridge_rust_release_strong(strong_id: u64, strong_type_id: u32);
        fn bridge_test_register_rust_object(type_id: u32) -> BridgeStrongHandle;
    }
}

pub const RUST_OBJECT_ID_BIT: u64 = 1 << 63;

struct RustBridgeEntry {
    type_id: u32,
    object: Arc<dyn Any + Send + Sync>,
}

struct RustBridgeRegistry {
    next_id: AtomicU64,
    strongs: Mutex<HashMap<u64, RustBridgeEntry>>,
    weaks: Mutex<HashMap<u64, (u32, Weak<dyn Any + Send + Sync>)>>,
}

impl RustBridgeRegistry {
    fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            strongs: Mutex::new(HashMap::new()),
            weaks: Mutex::new(HashMap::new()),
        }
    }
}

fn registry() -> &'static RustBridgeRegistry {
    static REGISTRY: OnceLock<RustBridgeRegistry> = OnceLock::new();
    REGISTRY.get_or_init(RustBridgeRegistry::new)
}

pub fn is_rust_owned_id(id: u64) -> bool {
    (id & RUST_OBJECT_ID_BIT) != 0
}

pub fn register_rust_owned<T>(type_id: u32, object: Arc<T>) -> ffi::BridgeStrongHandle
where
    T: Any + Send + Sync + 'static,
{
    let erased: Arc<dyn Any + Send + Sync> = object;
    let local_id = registry().next_id.fetch_add(1, Ordering::SeqCst);
    let id = RUST_OBJECT_ID_BIT | local_id;

    registry()
        .weaks
        .lock()
        .expect("Rust bridge weak registry poisoned")
        .insert(id, (type_id, Arc::downgrade(&erased)));
    registry()
        .strongs
        .lock()
        .expect("Rust bridge strong registry poisoned")
        .insert(id, RustBridgeEntry { type_id, object: erased });

    ffi::BridgeStrongHandle { id, type_id }
}

pub fn downgrade_strong(strong: ffi::BridgeStrongHandle) -> ffi::BridgeWeakHandle {
    ffi::BridgeWeakHandle {
        id: strong.id,
        type_id: strong.type_id,
    }
}

pub fn upgrade_weak(weak: ffi::BridgeWeakHandle) -> Option<ffi::BridgeStrongHandle> {
    if weak.id == 0 {
        return None;
    }

    if is_rust_owned_id(weak.id) {
        if bridge_rust_upgrade(weak.id, weak.type_id) {
            Some(ffi::BridgeStrongHandle {
                id: weak.id,
                type_id: weak.type_id,
            })
        } else {
            None
        }
    } else if ffi::bridge_upgrade_for_rust(weak.id, weak.type_id) {
        Some(ffi::BridgeStrongHandle {
            id: weak.id,
            type_id: weak.type_id,
        })
    } else {
        None
    }
}

pub fn release_strong(strong: ffi::BridgeStrongHandle) {
    if strong.id == 0 {
        return;
    }

    if is_rust_owned_id(strong.id) {
        bridge_rust_release_strong(strong.id, strong.type_id);
    } else {
        ffi::bridge_release_strong_for_rust(strong.id, strong.type_id);
    }
}

pub fn resolve_rust_owned<T>(weak: ffi::BridgeWeakHandle) -> Option<Arc<T>>
where
    T: Any + Send + Sync + 'static,
{
    if !is_rust_owned_id(weak.id) {
        return None;
    }

    let candidate = {
        let weaks = registry()
            .weaks
            .lock()
            .expect("Rust bridge weak registry poisoned");
        let (type_id, weak_obj) = weaks.get(&weak.id)?;
        if *type_id != weak.type_id {
            return None;
        }
        weak_obj.upgrade()?
    };

    candidate.downcast::<T>().ok()
}

fn bridge_is_rust_owned(id: u64) -> bool {
    is_rust_owned_id(id)
}

fn bridge_rust_upgrade(weak_id: u64, weak_type_id: u32) -> bool {
    if weak_id == 0 || !is_rust_owned_id(weak_id) {
        return false;
    }

    let upgraded = {
        let weaks = registry()
            .weaks
            .lock()
            .expect("Rust bridge weak registry poisoned");
        let Some((type_id, weak_obj)) = weaks.get(&weak_id) else {
            return false;
        };
        if *type_id != weak_type_id {
            return false;
        }
        weak_obj.upgrade()
    };

    if let Some(object) = upgraded {
        registry()
            .strongs
            .lock()
            .expect("Rust bridge strong registry poisoned")
            .insert(
                weak_id,
                RustBridgeEntry {
                    type_id: weak_type_id,
                    object,
                },
            );
        true
    } else {
        false
    }
}

fn bridge_rust_release_strong(strong_id: u64, strong_type_id: u32) {
    if strong_id == 0 || !is_rust_owned_id(strong_id) {
        return;
    }

    let mut strongs = registry()
        .strongs
        .lock()
        .expect("Rust bridge strong registry poisoned");
    if let Some(entry) = strongs.get(&strong_id) {
        if entry.type_id == strong_type_id {
            strongs.remove(&strong_id);
        }
    }
}

#[derive(Debug)]
struct BridgeTestObject {
    value: u32,
}

fn bridge_test_register_rust_object(type_id: u32) -> ffi::BridgeStrongHandle {
    register_rust_owned(type_id, Arc::new(BridgeTestObject { value: 42 }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const TEST_TYPE: u32 = 10_001;
    const OTHER_TYPE: u32 = 10_002;

    #[derive(Debug)]
    struct TestObject {
        value: u32,
    }

    #[test]
    fn downgrade_strong_copies_id_and_type_id() {
        let strong = ffi::BridgeStrongHandle { id: 123, type_id: 2 };
        let weak = downgrade_strong(strong);
        assert_eq!(weak.id, strong.id);
        assert_eq!(weak.type_id, strong.type_id);
    }

    #[test]
    fn handles_work_in_vec_and_hashset() {
        let weak = ffi::BridgeWeakHandle { id: 123, type_id: 2 };
        let v = vec![weak, weak];
        assert_eq!(v.len(), 2);
        assert_eq!(v[0], weak);

        let mut set = HashSet::new();
        set.insert(weak);
        set.insert(weak);
        assert_eq!(set.len(), 1);
        assert!(set.contains(&weak));
    }

    #[test]
    fn register_rust_owned_object_sets_owner_bit_and_resolves_typed_object() {
        let object = Arc::new(TestObject { value: 7 });
        let strong = register_rust_owned(TEST_TYPE, object.clone());
        let weak = downgrade_strong(strong);

        assert!(is_rust_owned_id(strong.id));
        assert_eq!(strong.type_id, TEST_TYPE);

        let resolved = resolve_rust_owned::<TestObject>(weak).unwrap();
        assert_eq!(resolved.value, 7);
        assert!(Arc::ptr_eq(&resolved, &object));

        release_strong(strong);
    }

    #[test]
    fn rust_owned_upgrade_and_release_lifecycle() {
        let object = Arc::new(TestObject { value: 9 });
        let strong = register_rust_owned(TEST_TYPE, object.clone());
        let weak = downgrade_strong(strong);

        release_strong(strong);
        assert!(upgrade_weak(weak).is_some());
        assert!(resolve_rust_owned::<TestObject>(weak).is_some());

        drop(object);
        release_strong(strong);
        assert!(upgrade_weak(weak).is_none());
    }

    #[test]
    fn rust_owned_type_mismatch_fails() {
        let strong = register_rust_owned(TEST_TYPE, Arc::new(TestObject { value: 11 }));
        let wrong = ffi::BridgeWeakHandle {
            id: strong.id,
            type_id: OTHER_TYPE,
        };

        assert!(upgrade_weak(wrong).is_none());
        assert!(resolve_rust_owned::<TestObject>(wrong).is_none());

        release_strong(strong);
    }

    #[test]
    fn stale_rust_owned_weak_returns_none() {
        let strong = register_rust_owned(TEST_TYPE, Arc::new(TestObject { value: 12 }));
        let weak = downgrade_strong(strong);
        release_strong(strong);

        assert!(upgrade_weak(weak).is_none());
        assert!(resolve_rust_owned::<TestObject>(weak).is_none());
    }

    #[test]
    fn upgrade_zero_short_circuits_without_cpp_call() {
        let zero = ffi::BridgeWeakHandle { id: 0, type_id: 0 };
        assert!(upgrade_weak(zero).is_none());
    }
}
