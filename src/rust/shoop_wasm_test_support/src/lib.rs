extern crate self as shoop_wasm_test_support;

pub use shoop_test_macros::shoop_test;

#[cfg(not(target_arch = "wasm32"))]
pub use futures::executor::block_on;
#[cfg(not(target_arch = "wasm32"))]
pub use shoop_tracing::{run_test, run_test_result};

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
pub use shoop_tracing::{wasm_test_trace_begin, wasm_test_trace_finish};

#[cfg(target_arch = "wasm32")]
pub fn wasm_test_trace_finish_result<T, E>(result: &Result<T, E>) {
    shoop_tracing::wasm_test_trace_finish_result(result.is_err());
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
    fn shared_result_test_runs() -> Result<(), &'static str> {
        Ok(())
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

    #[shoop_test]
    #[cfg_attr(
        not(feature = "wasm-test-failure-canary"),
        ignore = "opt-in Result failure canary"
    )]
    fn shared_result_failure_canary_is_ignored_by_default() -> Result<(), &'static str> {
        Err("intentional shared Result failure canary")
    }

    #[shoop_test(no_wasm = "exercises the native-only expansion")]
    fn native_only_modifier_runs() {
        assert!(cfg!(not(target_arch = "wasm32")));
    }

    #[shoop_test(no_trace = "exercises the uninstrumented native expansion")]
    fn no_trace_modifier_runs() {
        assert_eq!(6 * 7, 42);
        assert!(!shoop_tracing::is_tracing_enabled());
        assert!(!shoop_tracing::is_engine_detail_enabled());
    }

    #[shoop_test(no_wasm = "exercises native Result error preservation")]
    #[ignore = "opt-in Result failure canary"]
    fn native_result_failure_canary() -> Result<(), &'static str> {
        Err("intentional native Result failure")
    }

    #[shoop_test(
        no_wasm = "exercises native unwind capture internals",
        no_trace = "starts its own per-test capture"
    )]
    fn native_capture_runtime_preserves_panics() {
        let output = tempfile::tempdir().unwrap();
        let under_nextest = std::env::var_os("NEXTEST_ATTEMPT_ID").is_some();
        std::env::set_var("SHOOP_TEST_TRACE", "failure");
        std::env::set_var("SHOOP_TEST_TRACE_DIR", output.path());
        let outcome = std::panic::catch_unwind(|| {
            crate::run_test(|| panic!("intentional nested capture panic"));
        });
        assert!(outcome.is_err());
        assert_eq!(
            output.path().read_dir().unwrap().count(),
            usize::from(under_nextest)
        );
    }

    #[shoop_test(
        no_wasm = "exercises native Result capture internals",
        no_trace = "starts its own per-test capture"
    )]
    fn native_capture_runtime_preserves_result_errors() {
        let output = tempfile::tempdir().unwrap();
        let under_nextest = std::env::var_os("NEXTEST_ATTEMPT_ID").is_some();
        std::env::set_var("SHOOP_TEST_TRACE", "failure");
        std::env::set_var("SHOOP_TEST_TRACE_DIR", output.path());
        let result = crate::run_test_result(|| Err::<(), _>("intentional nested Result error"));
        assert_eq!(result, Err("intentional nested Result error"));
        assert_eq!(
            output.path().read_dir().unwrap().count(),
            usize::from(under_nextest)
        );
    }

    #[shoop_test(
        no_wasm = "exercises native process-wide trace dispatch",
        no_trace = "starts its own per-test capture"
    )]
    fn native_capture_reaches_worker_threads() {
        let output = tempfile::tempdir().unwrap();
        let under_nextest = std::env::var_os("NEXTEST_ATTEMPT_ID").is_some();
        std::env::set_var("SHOOP_TEST_TRACE", "always");
        std::env::set_var("SHOOP_TEST_TRACE_DIR", output.path());
        crate::run_test(|| {
            std::thread::spawn(|| {
                tracing::info!("shoop.test_capture.worker_thread_marker");
            })
            .join()
            .unwrap();
        });
        let traces = output.path().read_dir().unwrap().collect::<Vec<_>>();
        assert_eq!(traces.len(), usize::from(under_nextest));
        if let Some(trace) = traces.first() {
            let bytes = std::fs::read(trace.as_ref().unwrap().path()).unwrap();
            assert!(bytes
                .windows(b"shoop.test_capture.worker_thread_marker".len())
                .any(|window| window == b"shoop.test_capture.worker_thread_marker"));
        }
    }

    #[shoop_test(wasm_only = "exercises the Wasm-only expansion")]
    fn wasm_only_modifier_runs() {
        assert!(cfg!(target_arch = "wasm32"));
    }
}
