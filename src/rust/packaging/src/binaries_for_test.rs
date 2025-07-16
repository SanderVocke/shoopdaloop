use crate::dependencies::get_dependency_libs;
use anyhow;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::logging::macros::*;
shoop_log_unit!("packaging");

fn populate_folder(folder: &Path, release: bool) -> Result<(), anyhow::Error> {
    // For normalizing Windows paths
    let normalize_path = |path: PathBuf| -> PathBuf {
        PathBuf::from(
            std::fs::canonicalize(path)
                .unwrap()
                .to_str()
                .unwrap()
                .trim_start_matches(r"\\?\"),
        )
    };

    let file_path = PathBuf::from(file!());
    let src_path = normalize_path(file_path);
    let src_path = src_path
        .ancestors()
        .nth(5)
        .ok_or(anyhow::anyhow!("cannot find src dir"))?;
    info!("Using source path {src_path:?}");

    let testrunner_filename = if cfg![target_os = "windows"] {
        "test_runner.exe"
    } else {
        "test_runner"
    };
    let installed_testrunner = folder.join(testrunner_filename);
    for f in glob::glob(
        format!(
            "{}/**/test_runner*",
            backend::backend_build_dir().to_string_lossy().to_string()
        )
        .as_str(),
    )? {
        let f = f?.clone();
        info!("Bundling {f:?}...");
        std::fs::copy(&f, &installed_testrunner)?;
    }

    info!("Getting dependencies (this may take some time)...");
    let excludelist_path = if cfg!(target_os = "linux") {
        src_path.join("distribution/linux/testrunner_excludelist")
    } else if cfg!(target_os = "windows") {
        src_path.join("distribution/windows/testrunner_excludelist")
    } else if cfg!(target_os = "macos") {
        src_path.join("distribution/macos/testrunner_excludelist")
    } else {
        panic!()
    };
    let includelist_path = if cfg!(target_os = "linux") {
        src_path.join("distribution/linux/testrunner_includelist")
    } else if cfg!(target_os = "windows") {
        src_path.join("distribution/windows/testrunner_includelist")
    } else if cfg!(target_os = "macos") {
        src_path.join("distribution/macos/testrunner_includelist")
    } else {
        panic!()
    };

    for path in backend::runtime_link_dirs() {
        debug!("--> extra search path: {:?}", path);
        common::env::add_lib_search_path(&path);
    }
    let dependency_libs = get_dependency_libs(
        &installed_testrunner,
        folder,
        &excludelist_path,
        &includelist_path,
        false,
    )?;

    info!("Bundling {} dependencies...", dependency_libs.len());
    for lib in dependency_libs {
        let src = lib.clone();
        let dst = folder.join(lib.file_name().unwrap());
        debug!("--> {:?} -> {:?}", &src, &dst);
        std::fs::copy(&src, &dst)?;
    }

    info!("Downloading prebuilt cargo-nextest into folder...");

    let nextest_path: PathBuf;
    let nextest_dir = folder.to_str().unwrap();
    #[cfg(target_os = "windows")]
    {
        nextest_path = folder.join("cargo-nextest.exe");
        Command::new("powershell")
                .current_dir(&src_path)
                .args(&["-Command",
                        &format!("Invoke-WebRequest -Uri \"https://get.nexte.st/latest/windows\" -OutFile \"$env:TEMP\\nextest.zip\"; Expand-Archive -Path \"$env:TEMP\\nextest.zip\" -DestinationPath \"{}\" -Force; Remove-Item \"$env:TEMP\\nextest.zip\"",
                         nextest_dir)
                        ])
                .status()?;
    }
    #[cfg(target_os = "macos")]
    {
        nextest_path = folder.join("cargo-nextest");
        Command::new("sh")
            .current_dir(&src_path)
            .args(&[
                "-c",
                &format!(
                    "curl -LsSf https://get.nexte.st/latest/mac | tar zxf - -C \"{}\"",
                    nextest_dir
                ),
            ])
            .status()?;
    }
    #[cfg(target_os = "linux")]
    {
        nextest_path = folder.join("cargo-nextest");
        Command::new("sh")
            .current_dir(&src_path)
            .args(&[
                "-c",
                &format!(
                    "curl -LsSf https://get.nexte.st/latest/linux | tar zxf - -C \"{}\"",
                    nextest_dir
                ),
            ])
            .status()?;
    }

    info!("Creating nextest archive...");
    let archive = folder.join("nextest-archive.tar.zst");
    let args = match release {
        true => vec![
            "nextest",
            "archive",
            "--profile",
            "release-with-debug",
            "--archive-file",
            archive.to_str().unwrap(),
        ],
        false => vec![
            "nextest",
            "archive",
            "--archive-file",
            archive.to_str().unwrap(),
        ],
    };
    Command::new(&nextest_path)
        .current_dir(&src_path)
        .args(&args[..])
        .status()?;

    info!(
        "Test binaries folder produced in {}",
        folder.to_str().unwrap()
    );

    Ok(())
}

pub fn build_test_binaries_folder(output_dir: &Path, release: bool) -> Result<(), anyhow::Error> {
    if output_dir.exists() {
        return Err(anyhow::anyhow!(
            "Output directory {:?} already exists",
            output_dir
        ));
    }
    if !output_dir
        .parent()
        .ok_or(anyhow::anyhow!("Cannot find parent of {output_dir:?}"))?
        .exists()
    {
        return Err(anyhow::anyhow!(
            "Output directory {:?}: parent doesn't exist",
            output_dir
        ));
    }
    info!("Creating test binaries directory...");
    std::fs::create_dir(output_dir)?;

    populate_folder(output_dir, release)?;

    info!("Test binaries folder created @ {output_dir:?}");
    Ok(())
}
