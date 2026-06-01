#![allow(dead_code)]

#[cxx::bridge(namespace = "backend_rust")]
pub mod ffi {
    #[derive(Clone, Copy, Debug)]
    struct BridgeStrongHandle {
        id: u64,
        type_id: u32,
    }

    #[derive(Clone, Copy, Debug)]
    struct BridgeWeakHandle {
        id: u64,
        type_id: u32,
    }
}
