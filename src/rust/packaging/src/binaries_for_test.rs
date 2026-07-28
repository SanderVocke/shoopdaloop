use anyhow::anyhow;
use std::path::Path;

pub fn build_test_binaries_folder(
    _folder: &Path,
    _cargo_profile: &str,
) -> Result<(), anyhow::Error> {
    Err(anyhow!(
        "C/C++ backend test binaries are no longer built; use the Rust shoop_engine tests instead"
    ))
}
