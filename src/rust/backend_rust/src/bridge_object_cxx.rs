#![allow(dead_code)]

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

    let upgraded = ffi::bridge_upgrade_for_rust(weak.id, weak.type_id);
    if upgraded {
        Some(ffi::BridgeStrongHandle {
            id: weak.id,
            type_id: weak.type_id,
        })
    } else {
        None
    }
}

pub fn release_strong(strong: ffi::BridgeStrongHandle) {
    if strong.id != 0 {
        ffi::bridge_release_strong_for_rust(strong.id, strong.type_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

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
    fn upgrade_zero_short_circuits_without_cpp_call() {
        let zero = ffi::BridgeWeakHandle { id: 0, type_id: 0 };
        assert!(upgrade_weak(zero).is_none());
    }
}
