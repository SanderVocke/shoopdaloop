use std::process::Command;
use std::env;
use std::path::PathBuf;
use glob::glob;
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
        let is_debug_build = std::env::var("PROFILE").unwrap() == "debug";
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let src_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let base_src_dir = src_dir.parent().unwrap()
                                  .parent().unwrap()
                                  .parent().unwrap();
        let pyo3_python = env::var("PYO3_PYTHON");
        let dev_env_python = env::var("SHOOP_DEV_ENV_PYTHON")
                          .unwrap_or("python3".to_string());
        let python_src_dir = base_src_dir.join("src").join("python");

        println!("Using dev env Python: {}", dev_env_python);

        // Create a runtime venv using the dev env python to install our wheel into.
        let dev_venv_dir = out_dir.join("dev_venv");
        let dev_env_python_path : PathBuf = (&dev_env_python).into();
        let python_filename = dev_env_python_path.file_name().unwrap().to_string_lossy();
        let python_in_venv =
             if cfg!(target_os = "windows") { format!("Scripts/{python_filename}") }
             else { format!("bin/{python_filename}") };
        if dev_venv_dir.exists() {
            std::fs::remove_dir_all(&dev_venv_dir)
                .with_context(|| format!("Failed to remove development venv: {:?}", &dev_venv_dir))?;
        }
        println!("Creating development venv...");
        let args = &["-m", "venv", "--clear", "--system-site-packages", &dev_venv_dir.to_str().unwrap()];
        Command::new(&dev_env_python)
            .args(args)
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()
            .with_context(|| format!("Failed to create development venv: {dev_env_python:?} {args:?}"))?;
        let development_venv_python = dev_venv_dir.join(&python_in_venv);

        // Create a build-time venv using the dev env python to build our wheel with.
        let build_venv_dir = out_dir.join("build_venv");
        if build_venv_dir.exists() {
            std::fs::remove_dir_all(&build_venv_dir)
                .with_context(|| format!("Failed to remove build venv: {:?}", &build_venv_dir))?;
        }
        println!("Creating build venv...");
        let args = &["-m", "venv", "--clear", "--system-site-packages", &build_venv_dir.to_str().unwrap()];
        Command::new(&dev_env_python)
            .args(args)
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()
            .with_context(|| format!("Failed to create build venv: {dev_env_python:?} {args:?}"))?;
        // Install build requirements into the build venv
        println!("Installing build requirements...");
        let build_requirements_file = base_src_dir.join("build_python_requirements.txt");
        let args = &["-m", "pip", "install", "-r", build_requirements_file.to_str().unwrap()];
        let build_venv_python = build_venv_dir.join(&python_in_venv);
        Command::new(&build_venv_python)
            .args(args)
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()
            .with_context(|| format!("Failed to install build requirements: {build_venv_python:?} {args:?}"))?;

        // Build ShoopDaLoop wheel
        println!("Building wheel...");
        let args = &["-m", "build",
            "--outdir", out_dir.to_str().expect("Couldn't get out dir"),
            "--wheel",
            "--no-isolation",
            python_src_dir.to_str().unwrap()];
        Command::new(&build_venv_python)
            .args(args)
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .current_dir(out_dir.clone())
            .status()
            .with_context(|| format!("Failed to build wheel: {build_venv_python:?} {args:?}"))?;

        let wheel : PathBuf;
        {
            let whl_pattern = format!("{}/*.whl", out_dir.to_str().unwrap());
            let mut whl_glob = glob(&whl_pattern)?;
            wheel = whl_glob.next()
                    .expect(format!("No wheel found @ {}", whl_pattern).as_str())
                    .with_context(|| "Failed to glob for wheel")?;
            println!("Found built wheel: {}", wheel.to_str().unwrap());
        }

        println!("Using development Python wrapper: {}", development_venv_python.to_str().unwrap());
        // Install ShoopDaLoop wheel in env
        println!("Installing shoopdaloop and dependencies into python env...");
        // if !Command::new(&env_python)
        //     .args(
        //         &["-m", "ensurepip", "--upgrade"]
        //         // &["-m", "pip", "install", "--break-system-packages", "--no-input", "--upgrade", "pip"]
        //     )
        //     .stdout(std::process::Stdio::inherit())
        //     .stderr(std::process::Stdio::inherit())
        //     .status()?
        //     .success()
        // {
        //     return Err(anyhow::anyhow!("Failed to upgrade pip"));
        // }
        if !Command::new(&development_venv_python)
            .args(
                &["-m", "pip", "install", "--no-input", "pytest"]
            )
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()?
            .success()
        {
            return Err(anyhow::anyhow!("Failed to install pytest into development venv"));
        }
        if !Command::new(&development_venv_python)
            .args(
                &["-m", "pip", "install", "--no-input", "--force-reinstall", wheel.to_str().expect("Could not get wheel path")]
            )
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()?
            .success()
        {
            return Err(anyhow::anyhow!("Failed to install shoopdaloop wheel into development venv"));
        }

        // Determine the default PYTHONPATH for the dev environment
        // python -c "import sys; import os; print(os.pathsep.join(sys.path))"
        let dev_env_pythonpath = Command::new(&development_venv_python)
            .args(
                &["-c", "import sys; import os; print(os.pathsep.join(sys.path))"]
            )
            .stderr(std::process::Stdio::inherit())
            .output()?;
        let dev_env_pythonpath = String::from_utf8(dev_env_pythonpath.stdout).expect("Failed to decode output");

        // Rebuild if changed
        println!("cargo:rerun-if-changed=build.rs");
        println!("cargo:rerun-if-changed=src");
        println!("cargo:rerun-if-changed={}/shoopdaloop", python_src_dir.to_str().unwrap());
        println!("cargo:rerun-if-changed={}/pyproject.toml", python_src_dir.to_str().unwrap());
        println!("cargo:rerun-if-env-changed=SHOOP_DEV_ENV_PYTHON");

        println!("cargo:rustc-env=SHOOP_DEV_VENV_DIR={}", dev_venv_dir.to_str().unwrap());
        println!("cargo:rustc-env=SHOOPDALOOP_WHEEL={}", wheel.to_str().unwrap());
        println!("cargo:rustc-env=SHOOP_DEV_ENV_PYTHON={}", dev_env_python);
        println!("cargo:rustc-env=SHOOP_DEV_ENV_PYTHONPATH={}", dev_env_pythonpath);

        println!("build.rs finished.");

        Ok(())
    }
}

fn main() {
    match main_impl() {
        Ok(_) => {},
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}