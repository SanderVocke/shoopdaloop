use crate::SimResult;
use std::{ffi::OsStr, process::Command};

pub const SUPPORTED_VERILATOR: &str = "5.032";

pub fn check_verilator(executable: &OsStr) -> SimResult<()> {
    let output = Command::new(executable)
        .arg("--version")
        .output()
        .map_err(|error| {
            format!(
                "cannot run Verilator {}: {error}; install Verilator {SUPPORTED_VERILATOR}",
                executable.to_string_lossy()
            )
        })?;
    if !output.status.success() {
        return Err(format!(
            "Verilator --version failed ({}): {}{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    validate_version(&String::from_utf8_lossy(&output.stdout))
}

fn validate_version(output: &str) -> SimResult<()> {
    let mut words = output.split_whitespace();
    if words.next() != Some("Verilator") || words.next() != Some(SUPPORTED_VERILATOR) {
        return Err(format!(
            "unsupported Verilator version: {}; the FPGA RTL backend requires Verilator {SUPPORTED_VERILATOR}",
            output.trim()
        )
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;

    #[shoop_test(no_wasm = "native simulator toolchain")]
    fn only_the_validated_verilator_version_is_accepted() {
        assert!(validate_version("Verilator 5.032 2025-01-01 rev v5.032").is_ok());
        for version in ["Verilator 5.050", "Verilator 5.030", "", "5.032"] {
            let error = validate_version(version).unwrap_err().to_string();
            assert!(error.contains("requires Verilator 5.032"), "{error}");
        }
    }
}
