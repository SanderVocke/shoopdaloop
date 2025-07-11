use anyhow;
use anyhow::Context;
use copy_dir::copy_dir;
use glob::glob;
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
    let python_src_dir = base_src_dir.join("src").join("python");

    println!("Using dev env Python: {}", dev_env_python);

    // Set up environment variables in order to prevent generating files
    // into the source tree.
    let pycache_dir_str = out_dir.join("pycache").to_string_lossy().to_string();
    std::env::set_var("PYTHONPYCACHEPREFIX", pycache_dir_str.as_str());

    // Create a runtime venv using the dev env python to install our wheel into.
    let dev_venv_dir = out_dir.join("dev_venv");
    let dev_env_python_path: PathBuf = (&dev_env_python).into();
    let python_filename = dev_env_python_path.file_name().unwrap().to_string_lossy();
    let python_in_venv = if cfg!(target_os = "windows") {
        format!("Scripts/{python_filename}")
    } else {
        format!("bin/{python_filename}")
    };
    if dev_venv_dir.exists() {
        std::fs::remove_dir_all(&dev_venv_dir)
            .with_context(|| format!("Failed to remove development venv: {:?}", &dev_venv_dir))?;
    }
    println!("Creating development venv...");
    let args = &[
        "-m",
        "venv",
        "--clear",
        "--system-site-packages",
        &dev_venv_dir.to_str().unwrap(),
    ];
    Command::new(&dev_env_python)
        .args(args)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .with_context(|| {
            format!("Failed to create development venv: {dev_env_python:?} {args:?}")
        })?;
    let development_venv_python = dev_venv_dir.join(&python_in_venv);

    // Copy python source out-of-tree to prevent polluting source files
    // and/or race conditions
    println!("Copying python sources...");
    let python_build_src_dir = out_dir.join("py_src");
    if std::fs::exists(&python_build_src_dir)? {
        std::fs::remove_dir_all(&python_build_src_dir)?;
    }
    copy_dir(&python_src_dir, &python_build_src_dir)?;

    let build_venv_python = py_prepare::build_venv_python();

    // Build ShoopDaLoop wheel
    println!("Building wheel...");
    let args = &[
        "-m",
        "build",
        "--outdir",
        out_dir.to_str().expect("Couldn't get out dir"),
        "--wheel",
        "--no-isolation",
        python_build_src_dir.to_str().unwrap(),
    ];
    Command::new(&build_venv_python)
        .args(args)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .current_dir(out_dir.clone())
        .status()
        .with_context(|| format!("Failed to build wheel: {build_venv_python:?} {args:?}"))?;

    let wheel: PathBuf;
    {
        let whl_pattern = format!("{}/*.whl", out_dir.to_str().unwrap());
        let mut whl_glob = glob(&whl_pattern)?;
        wheel = whl_glob
            .next()
            .expect(format!("No wheel found @ {}", whl_pattern).as_str())
            .with_context(|| "Failed to glob for wheel")?;
        println!("Found built wheel: {}", wheel.to_str().unwrap());
    }

    println!(
        "Using development Python wrapper: {}",
        development_venv_python.to_str().unwrap()
    );
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
        .args(&["-m", "pip", "install", "--no-input", "pytest"])
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()?
        .success()
    {
        return Err(anyhow::anyhow!(
            "Failed to install pytest into development venv"
        ));
    }
    if !Command::new(&development_venv_python)
        .args(&[
            "-m",
            "pip",
            "install",
            "--no-input",
            "--force-reinstall",
            wheel.to_str().expect("Could not get wheel path"),
        ])
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()?
        .success()
    {
        return Err(anyhow::anyhow!(
            "Failed to install shoopdaloop wheel into development venv"
        ));
    }

    // Determine the default PYTHONPATH for the dev environment
    // python -c "import sys; import os; print(os.pathsep.join(sys.path))"
    let dev_env_pythonpath = Command::new(&development_venv_python)
        .args(&[
            "-c",
            "import sys; import os; print(os.pathsep.join(sys.path))",
        ])
        .stderr(std::process::Stdio::inherit())
        .output()?;
    let dev_env_pythonpath =
        String::from_utf8(dev_env_pythonpath.stdout).expect("Failed to decode output");

    // Rebuild if changed
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src");
    println!(
        "cargo:rerun-if-changed={}/shoopdaloop",
        python_src_dir.to_str().unwrap()
    );
    println!(
        "cargo:rerun-if-changed={}/pyproject.toml",
        python_src_dir.to_str().unwrap()
    );
    println!("cargo:rerun-if-env-changed=SHOOP_DEV_ENV_PYTHON");

    println!(
        "cargo:rustc-env=SHOOP_DEV_VENV_DIR={}",
        dev_venv_dir.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=SHOOPDALOOP_WHEEL={}",
        wheel.to_str().unwrap()
    );
    println!("cargo:rustc-env=SHOOP_DEV_ENV_PYTHON={}", dev_env_python);
    println!(
        "cargo:rustc-env=SHOOP_DEV_ENV_PYTHONPATH={}",
        dev_env_pythonpath
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
