//! CXX bridge used by C++ unit tests for Rust-native bridge objects.

#![allow(dead_code)]

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

static TEST_RUST_BRIDGE_OBJECT_DROPS: AtomicU32 = AtomicU32::new(0);

pub struct TestRustBridgeObject {
    value: AtomicU32,
}

impl TestRustBridgeObject {
    fn new() -> Self {
        Self {
            value: AtomicU32::new(0),
        }
    }

    fn increment(&self, amount: u32) {
        self.value.fetch_add(amount, Ordering::SeqCst);
    }

    fn value(&self) -> u32 {
        self.value.load(Ordering::SeqCst)
    }
}

impl Drop for TestRustBridgeObject {
    fn drop(&mut self) {
        TEST_RUST_BRIDGE_OBJECT_DROPS.fetch_add(1, Ordering::SeqCst);
    }
}

crate::define_rust_bridge_object_wrappers!(
    TestRustBridgeObjectBridgeStrong,
    TestRustBridgeObjectBridgeWeak,
    TestRustBridgeObject
);

#[cxx::bridge(namespace = "backend_rust")]
pub mod ffi {
    extern "Rust" {
        type TestRustBridgeObjectBridgeStrong;
        type TestRustBridgeObjectBridgeWeak;

        fn new_test_rust_bridge_object() -> Box<TestRustBridgeObjectBridgeStrong>;

        fn id(self: &TestRustBridgeObjectBridgeStrong) -> u64;
        fn valid(self: &TestRustBridgeObjectBridgeStrong) -> bool;
        fn downgrade(
            self: &TestRustBridgeObjectBridgeStrong,
        ) -> Box<TestRustBridgeObjectBridgeWeak>;
        fn clone_strong(
            self: &TestRustBridgeObjectBridgeStrong,
        ) -> Box<TestRustBridgeObjectBridgeStrong>;
        fn strong_count(self: &TestRustBridgeObjectBridgeStrong) -> usize;
        fn weak_count(self: &TestRustBridgeObjectBridgeStrong) -> usize;

        fn id(self: &TestRustBridgeObjectBridgeWeak) -> u64;
        fn valid(self: &TestRustBridgeObjectBridgeWeak) -> bool;
        fn upgrade(self: &TestRustBridgeObjectBridgeWeak) -> Box<TestRustBridgeObjectBridgeStrong>;
        fn clone(self: &TestRustBridgeObjectBridgeWeak) -> Box<TestRustBridgeObjectBridgeWeak>;
        fn strong_count(self: &TestRustBridgeObjectBridgeWeak) -> usize;
        fn weak_count(self: &TestRustBridgeObjectBridgeWeak) -> usize;

        fn test_rust_bridge_object_increment(
            object: &TestRustBridgeObjectBridgeWeak,
            amount: u32,
        ) -> bool;
        fn test_rust_bridge_object_value(object: &TestRustBridgeObjectBridgeWeak) -> u32;

        fn test_rust_bridge_object_reset_drop_count();
        fn test_rust_bridge_object_drop_count() -> u32;
    }
}

fn new_test_rust_bridge_object() -> Box<TestRustBridgeObjectBridgeStrong> {
    Box::new(TestRustBridgeObjectBridgeStrong::from_arc(Arc::new(
        TestRustBridgeObject::new(),
    )))
}

fn test_rust_bridge_object_increment(object: &TestRustBridgeObjectBridgeWeak, amount: u32) -> bool {
    object.0.with(|inner| inner.increment(amount)).is_some()
}

fn test_rust_bridge_object_value(object: &TestRustBridgeObjectBridgeWeak) -> u32 {
    object.0.with(TestRustBridgeObject::value).unwrap_or(0)
}

fn test_rust_bridge_object_reset_drop_count() {
    TEST_RUST_BRIDGE_OBJECT_DROPS.store(0, Ordering::SeqCst);
}

fn test_rust_bridge_object_drop_count() -> u32 {
    TEST_RUST_BRIDGE_OBJECT_DROPS.load(Ordering::SeqCst)
}
