use std::ffi::{OsStr, OsString};
use std::fmt::Debug;
use std::path::PathBuf;

use sha2::{Digest, Sha256};
use tracing_subscriber::layer::SubscriberExt;

use crate::capture::{CaptureDisposition, ReusableCaptureSession};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Policy {
    Off,
    Failure,
    Always,
}

struct TestCapture {
    policy: Policy,
    capture: ReusableCaptureSession,
    diagnostic_identity: String,
}

impl TestCapture {
    fn start() -> Result<Option<Self>, String> {
        let identity = (
            std::env::var_os("NEXTEST_ATTEMPT_ID"),
            std::env::var_os("NEXTEST_TEST_NAME"),
            std::env::var_os("NEXTEST_BINARY_ID"),
            std::env::var_os("NEXTEST_ATTEMPT"),
        );
        let (Some(attempt_id), Some(test_name), Some(binary), Some(attempt)) = identity else {
            return Ok(None);
        };
        let policy = parse_policy(std::env::var_os("SHOOP_TEST_TRACE"))?;
        if policy == Policy::Off {
            return Ok(None);
        }
        if !cfg!(panic = "unwind") {
            return Err("native per-test capture requires panic=unwind".to_owned());
        }
        let root = PathBuf::from(
            std::env::var_os("SHOOP_TEST_TRACE_DIR")
                .ok_or("SHOOP_TEST_TRACE_DIR is required when capture is enabled")?,
        );
        if !root.is_dir() {
            return Err(format!(
                "test trace output directory does not exist: {}",
                root.display()
            ));
        }
        let test_name = utf8(test_name, "NEXTEST_TEST_NAME")?;
        let binary = utf8(binary, "NEXTEST_BINARY_ID")?;
        let attempt = utf8(attempt, "NEXTEST_ATTEMPT")?;
        let label = format!(
            "{}--{}--attempt-{}--{}",
            sanitize(&binary),
            sanitize(&test_name),
            sanitize(&attempt),
            attempt_digest(&attempt_id),
        );
        let diagnostic_identity = format!(
            "test={} attempt={} id-digest={}",
            sanitize(&test_name),
            sanitize(&attempt),
            attempt_digest(&attempt_id),
        );
        let capture = ReusableCaptureSession::start(&root, &label)
            .map_err(|error| format!("{diagnostic_identity}: {error}"))?;
        crate::set_tracing_output_enabled(true);
        crate::set_tracing_enabled(true);
        crate::set_engine_detail_enabled(
            std::env::var_os("SHOOP_CI_TRACING_ENGINE_DETAIL").is_some(),
        );
        crate::emit_realtime_event("shoop.test_capture.attempt.begin");
        Ok(Some(Self {
            policy,
            capture,
            diagnostic_identity,
        }))
    }

    fn finish(mut self, failed: bool) -> Result<(), String> {
        crate::set_tracing_output_enabled(true);
        crate::set_tracing_enabled(true);
        if failed {
            crate::emit_realtime_event("shoop.test_capture.attempt.failure");
        } else {
            crate::emit_realtime_event("shoop.test_capture.attempt.success");
        }
        crate::set_tracing_output_enabled(false);
        crate::set_engine_detail_enabled(false);
        crate::set_tracing_enabled(false);
        let disposition = if failed || self.policy == Policy::Always {
            CaptureDisposition::Save
        } else {
            CaptureDisposition::Discard
        };
        self.capture
            .stop(disposition)
            .map_err(|error| format!("{}: {error}", self.diagnostic_identity))?;
        if disposition == CaptureDisposition::Save {
            eprintln!(
                "Perfetto test capture published: {}",
                self.capture.path().display()
            );
        }
        Ok(())
    }
}

/// Run one synchronous unit-returning native test with optional per-attempt capture.
pub fn run_test(body: impl FnOnce()) {
    let capture = TestCapture::start().unwrap_or_else(|error| configuration_failure(error));
    if capture.is_some() {
        install_process_test_subscriber();
    }
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body));
    match outcome {
        Ok(()) => {
            if let Some(capture) = capture {
                capture
                    .finish(false)
                    .unwrap_or_else(|error| configuration_failure(error));
            }
        }
        Err(payload) => {
            if let Some(capture) = capture {
                if let Err(error) = capture.finish(true) {
                    eprintln!("capture finalization failed while handling original panic: {error}");
                    std::process::exit(70);
                }
            }
            std::panic::resume_unwind(payload);
        }
    }
}

/// Run one synchronous Result-returning native test with optional per-attempt capture.
pub fn run_test_result<E: Debug>(body: impl FnOnce() -> Result<(), E>) -> Result<(), E> {
    let capture = TestCapture::start().unwrap_or_else(|error| configuration_failure(error));
    if capture.is_some() {
        install_process_test_subscriber();
    }
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body));
    match outcome {
        Ok(Ok(())) => {
            if let Some(capture) = capture {
                capture
                    .finish(false)
                    .unwrap_or_else(|error| configuration_failure(error));
            }
            Ok(())
        }
        Ok(Err(error)) => {
            if let Some(capture) = capture {
                if let Err(capture_error) = capture.finish(true) {
                    eprintln!(
                        "capture finalization failed while returning test error: {capture_error}; original error: {error:?}"
                    );
                    std::process::exit(70);
                }
            }
            Err(error)
        }
        Err(payload) => {
            if let Some(capture) = capture {
                if let Err(error) = capture.finish(true) {
                    eprintln!("capture finalization failed while handling original panic: {error}");
                    std::process::exit(70);
                }
            }
            std::panic::resume_unwind(payload);
        }
    }
}

fn install_process_test_subscriber() {
    let _ = tracing_log::LogTracer::init();
    let subscriber = tracing_subscriber::registry().with(crate::subscriber_layer());
    tracing::subscriber::set_global_default(subscriber).unwrap_or_else(|error| {
        configuration_failure(format!(
            "could not install process-wide Perfetto test subscriber: {error}"
        ))
    });
}

fn parse_policy(value: Option<OsString>) -> Result<Policy, String> {
    match value.as_deref().and_then(OsStr::to_str) {
        None | Some("off") => Ok(Policy::Off),
        Some("failure") => Ok(Policy::Failure),
        Some("always") => Ok(Policy::Always),
        Some(value) => Err(format!("invalid SHOOP_TEST_TRACE policy: {value}")),
    }
}

fn utf8(value: OsString, name: &str) -> Result<String, String> {
    value
        .into_string()
        .map_err(|_| format!("{name} must be valid UTF-8"))
}

fn sanitize(value: &str) -> String {
    let mut result = String::new();
    for character in value.chars().take(48) {
        result.push(
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            },
        );
    }
    if result.is_empty() {
        "test".to_owned()
    } else {
        result
    }
}

fn attempt_digest(attempt_id: &OsStr) -> String {
    let mut digest = Sha256::new();
    digest.update(attempt_id.to_string_lossy().as_bytes());
    digest
        .finalize()
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn configuration_failure(error: String) -> ! {
    eprintln!("Perfetto test capture configuration failed: {error}");
    std::process::exit(70)
}
