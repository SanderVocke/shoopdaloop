use anyhow;
use anyhow::Context;
use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main_impl() -> Result<(), anyhow::Error> {
    // If we're pre-building, don't do anything
    if cfg!(feature = "prebuild") {
        return Ok(());
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let src_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let base_src_dir = src_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let dev_env_python = env::var("SHOOP_DEV_ENV_PYTHON").unwrap_or("python3".to_string());
    let dev_env_python_path: PathBuf = (&dev_env_python).into();
    let python_filename = dev_env_python_path.file_name().unwrap().to_string_lossy();
    let python_in_venv = if cfg!(target_os = "windows") {
        format!("Scripts/{python_filename}")
    } else {
        format!("bin/{python_filename}")
    };

    println!("Using Python: {}", dev_env_python);

    // Create a build-time venv using the dev env python to build our wheel with.
    let build_venv_dir = out_dir.join("build_venv");
    if build_venv_dir.exists() {
        std::fs::remove_dir_all(&build_venv_dir)
            .with_context(|| format!("Failed to remove build venv: {:?}", &build_venv_dir))?;
    }
    println!("Creating build venv...");
    let args = &[
        "-m",
        "venv",
        "--clear",
        "--system-site-packages",
        &build_venv_dir.to_str().unwrap(),
    ];
    Command::new(&dev_env_python)
        .args(args)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .with_context(|| format!("Failed to create build venv: {dev_env_python:?} {args:?}"))?;
    // Install build requirements into the build venv
    println!("Installing build requirements...");
    let build_requirements_file = base_src_dir.join("build_python_requirements.txt");
    let args = &[
        "-m",
        "pip",
        "install",
        "-r",
        build_requirements_file.to_str().unwrap(),
    ];
    let build_venv_python = build_venv_dir.join(&python_in_venv);
    Command::new(&build_venv_python)
        .args(args)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .with_context(|| {
            format!("Failed to install build requirements: {build_venv_python:?} {args:?}")
        })?;

    // Rebuild if changed
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-env-changed=SHOOP_DEV_ENV_PYTHON");

    println!(
        "cargo:rustc-env=SHOOP_BUILD_VENV_DIR={}",
        build_venv_dir.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=SHOOP_BUILD_VENV_PYTHON={}",
        build_venv_python.to_string_lossy()
    );

    println!("build.rs finished.");

    Ok(())
}

fn main() {
    match main_impl() {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}
