use anyhow::{anyhow, Context};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use common::logging::macros::*;
shoop_log_unit!("packaging");

fn check_status(status: ExitStatus, operation: &str) -> Result<(), anyhow::Error> {
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("{operation} failed with status {status}"))
    }
}

#[tracing::instrument(name = "tool.packaging.populate_test_binaries", skip_all)]
fn populate_folder(folder: &Path, cargo_profile: &str) -> Result<(), anyhow::Error> {
    let source_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .ok_or_else(|| anyhow!("cannot find workspace root"))?
        .to_path_buf();
    info!("Using source path {source_root:?}");

    info!("Downloading cargo-nextest into test artifact...");
    let nextest_path: PathBuf;
    let destination = folder
        .to_str()
        .ok_or_else(|| anyhow!("Invalid unicode in test artifact path"))?;

    #[cfg(target_os = "windows")]
    {
        nextest_path = folder.join("cargo-nextest.exe");
        let status = Command::new("powershell")
            .current_dir(&source_root)
            .args([
                "-Command",
                &format!(
                    "Invoke-WebRequest -Uri \"https://get.nexte.st/latest/windows\" -OutFile \"$env:TEMP\\nextest.zip\"; Expand-Archive -Path \"$env:TEMP\\nextest.zip\" -DestinationPath \"{destination}\" -Force; Remove-Item \"$env:TEMP\\nextest.zip\""
                ),
            ])
            .status()
            .context("failed to launch cargo-nextest download")?;
        check_status(status, "cargo-nextest download")?;
    }
    #[cfg(target_os = "macos")]
    {
        nextest_path = folder.join("cargo-nextest");
        let status = Command::new("sh")
            .current_dir(&source_root)
            .args([
                "-c",
                &format!(
                    "curl -LsSf https://get.nexte.st/latest/mac | tar zxf - -C \"{destination}\""
                ),
            ])
            .status()
            .context("failed to launch cargo-nextest download")?;
        check_status(status, "cargo-nextest download")?;
    }
    #[cfg(target_os = "linux")]
    {
        nextest_path = folder.join("cargo-nextest");
        let status = Command::new("sh")
            .current_dir(&source_root)
            .args([
                "-c",
                &format!(
                    "curl -LsSf https://get.nexte.st/latest/linux | tar zxf - -C \"{destination}\""
                ),
            ])
            .status()
            .context("failed to launch cargo-nextest download")?;
        check_status(status, "cargo-nextest download")?;
    }

    {
        let _span = tracing::info_span!("tool.packaging.nextest_ready").entered();
        std::fs::metadata(&nextest_path).context("downloaded cargo-nextest is missing")?;
    }

    info!("Creating nextest archive for all workspace tests...");
    let archive = folder.join("nextest-archive.tar.zst");
    let status = {
        let _span = tracing::info_span!("tool.packaging.nextest_archive").entered();
        Command::new(&nextest_path)
            .current_dir(&source_root)
            .args([
                "nextest",
                "archive",
                "--workspace",
                "--features",
                "shoop_engine/app_backend",
                "--archive-file",
                archive
                    .to_str()
                    .ok_or_else(|| anyhow!("Invalid unicode in nextest archive path"))?,
                "--cargo-profile",
                cargo_profile,
            ])
            .status()
            .context("failed to launch cargo-nextest archive")?
    };
    check_status(status, "cargo-nextest archive")?;

    info!("Test artifact produced in {}", folder.display());
    Ok(())
}

#[tracing::instrument(name = "tool.packaging.build_test_binaries", skip_all)]
pub fn build_test_binaries_folder(
    output_dir: &Path,
    cargo_profile: &str,
) -> Result<(), anyhow::Error> {
    if output_dir.exists() {
        return Err(anyhow!("Output directory {output_dir:?} already exists"));
    }
    let parent = output_dir
        .parent()
        .ok_or_else(|| anyhow!("Cannot find parent of {output_dir:?}"))?;
    if !parent.exists() {
        return Err(anyhow!(
            "Output directory {output_dir:?}: parent does not exist"
        ));
    }

    info!("Creating Rust test artifact directory...");
    std::fs::create_dir(output_dir)?;
    populate_folder(output_dir, cargo_profile)
}
