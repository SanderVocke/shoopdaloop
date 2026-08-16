extern crate self as shoop_wasm_test_support;

pub use shoop_test_macros::shoop_test;

#[cfg(not(target_arch = "wasm32"))]
pub use futures::executor::block_on;
#[cfg(not(target_arch = "wasm32"))]
pub use tracy_nextest_capture::tracy_capture_test;

#[cfg(not(target_arch = "wasm32"))]
pub fn assert_panics(function: impl FnOnce(), expected: Option<&str>) {
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(function))
        .expect_err("test did not panic as expected");
    if let Some(expected) = expected {
        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .unwrap_or("<non-string panic>");
        assert!(
            message.contains(expected),
            "panic message {message:?} does not contain {expected:?}"
        );
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
wasm_bindgen_test_configure!(run_in_browser);

#[cfg(test)]
mod tests {
    use super::shoop_test;

    #[shoop_test]
    fn shared_sync_test_runs() {
        assert_eq!(2 + 2, 4);
    }

    #[shoop_test]
    async fn shared_async_test_runs() {
        let value = async { 42 }.await;
        assert_eq!(value, 42);
    }

    #[shoop_test]
    #[should_panic(expected = "expected shared panic")]
    fn shared_expected_panic_is_reported() {
        panic!("expected shared panic");
    }

    #[shoop_test]
    #[cfg_attr(
        not(feature = "wasm-test-failure-canary"),
        ignore = "opt-in failure canary"
    )]
    fn shared_failure_canary_is_ignored_by_default() {
        panic!("intentional shared test failure canary");
    }

    #[shoop_test(no_wasm = "exercises the native-only expansion")]
    fn native_only_modifier_runs() {
        assert!(cfg!(not(target_arch = "wasm32")));
    }

    #[shoop_test(no_tracy = "exercises the uninstrumented native expansion")]
    fn no_tracy_modifier_runs() {
        assert_eq!(6 * 7, 42);
    }

    #[shoop_test(wasm_only = "exercises the Wasm-only expansion")]
    fn wasm_only_modifier_runs() {
        assert!(cfg!(target_arch = "wasm32"));
    }
}
