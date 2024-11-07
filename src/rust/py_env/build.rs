use std::process::Command;
use std::env;
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use glob::glob;
use copy_dir::copy_dir;
use anyhow;
use anyhow::Context;

fn main_impl() -> Result<(), anyhow::Error> {
    // If we're pre-building, don't do anything
    #[cfg(feature = "prebuild")]
    {
        return Ok(());
    }

    #[cfg(not(feature = "prebuild"))]
    {
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let src_dir = env::current_dir()?;
        let host_python = env::var("PYO3_PYTHON")
                          .unwrap_or(
                             env::var("PYTHON")
                                .unwrap_or(String::from("python3")));
        let python_src_dir = format!("{}/../../python", src_dir.to_str().unwrap());

        println!("Using Python: {}", host_python);

        // Build ShoopDaLoop wheel
        println!("Building wheel...");
        let args = &["-m", "build",
            "--outdir", out_dir.to_str().expect("Couldn't get out dir"),
            "--wheel",
            &python_src_dir];
        Command::new(&host_python)
            .args(args)
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .current_dir(out_dir.clone())
            .status()
            .with_context(|| format!("Failed to build wheel: {host_python:?} {args:?}"))?;

        let wheel : PathBuf;
        {
            let whl_pattern = format!("{}/*.whl", out_dir.to_str().unwrap());
            let mut whl_glob = glob(&whl_pattern)?;
            wheel = whl_glob.next()
                    .expect(format!("No wheel found @ {}", whl_pattern).as_str())
                    .with_context(|| "Failed to glob for wheel")?;
            println!("Found built wheel: {}", wheel.to_str().unwrap());
        }
        let py_env_dir = Path::new(&out_dir).join("shoop_pyenv");
        let pyenv_root_dir = Path::new(&out_dir).join("pyenv_root");

        if py_env_dir.exists() {
            std::fs::remove_dir_all(&py_env_dir)
                .with_context(|| format!("Failed to remove Py env: {:?}", &py_env_dir))?;
        }
        if !pyenv_root_dir.exists() {
            std::fs::create_dir(&pyenv_root_dir)?;
        }

        // Create a env in OUT_DIR
        println!("Creating portable Python...");
        let py_version = Command::new(host_python)
                                .args(&["-c",
                                        "import sys; print(f'{sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}')"])
                                .output()
                                .with_context(|| format!("Failed to print python version: sh ${args:?}"))?;
        let py_version = std::str::from_utf8(&py_version.stdout)?;
        let py_version = py_version.trim();
        let mut py_location : String;
        println!("Using pyenv to install {} to {}...", py_version, pyenv_root_dir.to_str().unwrap());
        let args = &["install", "--skip-existing", py_version];
        let pyenv = env::var("PYENV").unwrap_or(String::from("pyenv"));
        let mut install_env : HashMap<String, String> = env::vars().collect();
        install_env.insert("PYENV_ROOT".to_string(), pyenv_root_dir.to_str().unwrap().to_string());
        install_env.insert("PYTHON_CONFIGURE_OPTS".to_string(), "--enable-shared".to_string());
        println!("   {pyenv:?} {args:?}");
        println!("   with PYENV_ROOT={pyenv_root_dir:?}");
        println!("   with PYTHON_CONFIGURE_OPTS=--enable-shared");
        Command::new(&pyenv)
                .args(args)
                .envs(install_env)
                .status()
                .with_context(|| format!("Failed to install Python using pyenv: {pyenv:?} {args:?}"))?;
        let py_location_output = Command::new(&pyenv)
                                        .args(&["prefix", &py_version])
                                        .env("PYENV_ROOT", &pyenv_root_dir)
                                        .output()
                                        .with_context(|| "Failed to get python location output using pyenv")?;
        py_location = String::from(std::str::from_utf8(&py_location_output.stdout)?);
        if !py_location_output.status.success() {
            let maybe_root = env::var("PYENV_ROOT");
            if maybe_root.is_ok() {
                let maybe_root = maybe_root.unwrap();
                let root = Path::new(&maybe_root);
                let maybe_py_location = root.join("versions").join(py_version);
                if maybe_py_location.exists() {
                    println!("pyenv prefix command failed. Using guessed prefix {maybe_py_location:?}");
                    py_location = String::from(maybe_py_location.to_str().unwrap());
                } else {
                    return Err(anyhow::anyhow!("Could not find Python location in pyenv root"));
                }
            } else {
                return Err(anyhow::anyhow!("Could not find Python location in pyenv root"));
            }
        }
        let py_location = py_location.trim();
        println!("Using Python from: {}. Installing to: {}", py_location, py_env_dir.to_str().unwrap());
        copy_dir(&py_location, &py_env_dir)
            .with_context(|| format!("Failed to copy Python env from {:?} to {:?}", &py_location, &py_env_dir))?;

        // Install ShoopDaLoop wheel in env
        let py_env_python : PathBuf;
        #[cfg(target_os = "windows")]
        {
            py_env_python = py_env_dir.join("python.exe").to_owned();
        }
        #[cfg(target_os = "linux")]
        {
            py_env_python = py_env_dir.join("bin").join("python").to_owned();
        }
        #[cfg(target_os = "macos")]
        {
            // Glob for **/bin/python and use it to set py_env_python
            let py_glob = format!("{}/**/bin/python", py_env_dir.to_str().unwrap());
            let mut py_glob = glob(&py_glob)?;
            py_env_python = py_glob.next()
                .expect("No python found")
                .with_context(|| "Failed to glob for python")?;
        }
        println!("Using installed Python interpreter: {}", py_env_python.to_str().unwrap());
        println!("Installing into python env...");
        Command::new(&py_env_python)
            .args(
                &["-m", "pip", "install", "--no-input", "--upgrade", "pip"]
            )
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()
            .with_context(|| "Failed to upgrade pip")?;
        Command::new(&py_env_python)
            .args(
                &["-m", "pip", "install", "--no-input", "pytest"]
            )
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()
            .with_context(|| "Failed to upgrade pip")?;
        Command::new(&py_env_python)
            .args(
                &["-m", "pip", "install", "--no-input", "--force-reinstall", wheel.to_str().expect("Could not get wheel path")]
            )
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()
            .with_context(|| "Failed to install wheel")?;

        // Tell PyO3 where to find our venv Python
        println!("Setting PYO3_PYTHON to {}", py_env_python.to_str().unwrap());
        env::set_var("PYO3_PYTHON", py_env_python.to_str().unwrap());
        println!("cargo:rustc-env=PYO3_PYTHON={}", py_env_python.to_str().unwrap());

        // Rebuild if changed
        println!("cargo:rerun-if-changed=build.rs");
        println!("cargo:rerun-if-changed=src");
        println!("cargo:rerun-if-changed={}/shoopdaloop", python_src_dir);
        println!("cargo:rerun-if-changed={}/pyproject.toml", python_src_dir);

        println!("cargo:rustc-env=SHOOP_PY_ENV_DIR={}", py_env_dir.to_str().unwrap());
        println!("cargo:rustc-env=SHOOP_PY_INTERPRETER={}", py_env_python.to_str().unwrap());

        println!("build.rs finished.");

        Ok(())
    }
}

fn main() {
    match main_impl() {
        Ok(_) => {},
        Err(e) => {
            eprintln!("Error: {:?}\nBacktrace: {:?}", e, e.backtrace());
            std::process::exit(1);
        }
    }
}