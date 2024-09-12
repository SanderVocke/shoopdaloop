use std::process::Command;
use std::env;
use std::path::{Path, PathBuf};
use glob::glob;
use cmake::Config;
use copy_dir::copy_dir;
use std::io;
use std::fs;
use std::io::prelude::*;
use std::hash::Hasher;
use anyhow;
use anyhow::Context;

fn file_hash<P: AsRef<Path>>(path: P) -> io::Result<u64> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let mut file = fs::File::open(path)?;
    let mut buffer = [0; 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.write(&buffer[..n]);
    }
    Ok(hasher.finish())
}

fn main_impl() -> Result<(), anyhow::Error> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let src_dir = env::current_dir()?;
    let host_python = env::var("PYTHON").unwrap_or(String::from("python3"));
    let shoop_lib_dir = out_dir.join("shoop_lib");
    let cmake_backend_dir = "../../backend";
    let python_dir = "../../python";
    let profile = std::env::var("PROFILE").unwrap();

    let build_whole_library = match env::var("CARGO_BIN_NAME").as_deref() {
        Err(_) => true,
        Ok("shoopdaloop") => false,
        Ok("shoopdaloop_dev") => false,
        _ => false
    };

    println!("HELLO {:?} {}", env::var("CARGO_BIN_NAME"), build_whole_library );

    if !["debug", "release"].contains(&profile.as_str()) {
        return Err(anyhow::anyhow!("Unknown build profile: {}", &profile));
    }

    println!("Using Python: {}", host_python);

    if build_whole_library {
        if shoop_lib_dir.exists() {
            std::fs::remove_dir_all(&shoop_lib_dir)
                .with_context(|| format!("Failed to remove {:?}", &shoop_lib_dir))?;
        }

        // Build back-end via CMake and install into our output directory
        println!("Building back-end...");
        let _ = Config::new(cmake_backend_dir)
            .out_dir(out_dir.join("cmake_build"))
            .generator("Ninja")
            .configure_arg(format!("-DCMAKE_INSTALL_PREFIX={}",shoop_lib_dir.to_str().unwrap()))
            .build();

        // Copy filesets into our output lib dir
        let to_copy = ["lua", "qml", "session_schemas", "../resources"];
        for directory in to_copy {
            let src = src_dir.join("../..").join(directory);
            let dst = shoop_lib_dir.join(PathBuf::from(directory).file_name().unwrap());
            copy_dir(&src, &dst)
                .expect(&format!("Failed to copy {} to {}",
                                src.display(),
                                dst.display()));
        }

        // Build ShoopDaLoop wheel
        println!("Building wheel...");
        let args = &["-m", "build",
            "--outdir", out_dir.to_str().expect("Couldn't get out dir"),
            "--wheel",
            python_dir];
        Command::new(&host_python)
            .args(args)
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .current_dir(src_dir.clone())
            .status()
            .with_context(|| format!("Failed to build wheel: {host_python:?} {args:?}"))?;
    }

    let wheel : PathBuf;
    {
        let whl_pattern = format!("{}/*.whl", out_dir.to_str().unwrap());
        let mut whl_glob = glob(&whl_pattern)?;
        wheel = whl_glob.next()
                .expect(format!("No wheel found @ {}", whl_pattern).as_str())
                .with_context(|| "Failed to glob for wheel")?;
        println!("Found built wheel: {}", wheel.to_str().unwrap());
    }
    // Use hash to determine if the wheel changed
    let wheel_hash = file_hash(&wheel).unwrap();
    let prev_wheel_file = out_dir.join("py_wheel_hash");
    let py_env_dir = Path::new(&out_dir).join("shoop_pyenv");
    let py_env_python = py_env_dir.join("bin").join("python");
    let pyenv_root_dir = Path::new(&out_dir).join("pyenv_root");

    let mut rebuild_env = build_whole_library;
    if prev_wheel_file.exists() && py_env_dir.exists() {
        let prev_wheel_hash = std::fs::read_to_string(&prev_wheel_file).unwrap().parse::<u64>()?;
        println!("Previous wheel hash: {}", prev_wheel_hash);
        println!("Current wheel hash: {}", wheel_hash);
        if prev_wheel_hash == wheel_hash {
            println!("Wheel unchanged, skipping env creation.");
            rebuild_env = false;
        }
    }

    if rebuild_env {
        if py_env_dir.exists() {
            std::fs::remove_dir_all(&py_env_dir)
              .with_context(|| format!("Failed to remove Py env: {:?}", &py_env_dir))?;
        }
        if !pyenv_root_dir.exists() {
            std::fs::create_dir(&pyenv_root_dir)?;
        }

        // Create a env in OUT_DIR
        println!("Creating portable Python...");
        let args = &["-c", "python3 --version | sed -r 's/.*3\\./3\\./g'"];
        let py_version = Command::new("sh")
                                .args(args)
                                .output()
                                .with_context(|| format!("Failed to print python version: sh ${args:?}"))?;
        let py_version = std::str::from_utf8(&py_version.stdout)?;
        let py_version = py_version.trim();
        println!("Using pyenv to install {} to {}...", py_version, pyenv_root_dir.to_str().unwrap());
        let args = &["install", "--skip-existing", py_version];
        let pyenv = env::var("PYENV").unwrap_or(String::from("pyenv"));
        Command::new(pyenv)
                .args(args)
                .env("PYENV_ROOT", &pyenv_root_dir)
                .status()
                .with_context(|| format!("Failed to install Python using pyenv: {pyenv:?} {args:?}"))?;
        let py_location = Command::new("pyenv")
                                     .args(&["prefix", &py_version])
                                     .env("PYENV_ROOT", &pyenv_root_dir)
                                     .output()
                                     .with_context(|| "Failed to get python location output using pyenv")?;
        let py_location = std::str::from_utf8(&py_location.stdout)
            .with_context(|| "Failed to parse python location output")?;
        let py_location = py_location.trim();
        println!("Using Python from: {}. Installing to: {}", py_location, py_env_dir.to_str().unwrap());
        copy_dir(&py_location, &py_env_dir)
            .with_context(|| format!("Failed to copy Python env from {:?} to {:?}", &py_location, &py_env_dir))?;

        // Install ShoopDaLoop wheel in env
        println!("Installing into python env...");
        Command::new(&py_env_python)
            .args(
                &["-m", "pip", "install", "--upgrade", "pip"]
            )
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()
            .with_context(|| "Failed to upgrade pip")?;
        Command::new(&py_env_python)
            .args(
                &["-m", "pip", "install", "--force-reinstall", wheel.to_str().expect("Could not get wheel path")]
            )
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()
            .with_context(|| "Failed to install wheel")?;

        std::fs::write(&prev_wheel_file, wheel_hash.to_string()).unwrap();
    }

    let py_env_to_site_packages : PathBuf;
    {
        let pattern = format!("{}/**/site-packages", py_env_dir.to_str().unwrap());
        let mut sp_glob = glob(&pattern).expect("Couldn't glob for site-packages");
        let full_site_packages = sp_glob.next()
                .expect(format!("No site-packages dir found @ {}", pattern).as_str()).unwrap();
        py_env_to_site_packages = PathBuf::from
                                (format!("lib/{}/site-packages",
                                 full_site_packages.parent().unwrap().file_name().unwrap().to_str().unwrap()));
    }

    // Tell PyO3 where to find our venv Python
    println!("Setting PYO3_PYTHON to {}", py_env_python.to_str().unwrap());
    env::set_var("PYO3_PYTHON", py_env_python.to_str().unwrap());

    // Set RPATH
    println!("cargo:rustc-link-arg-bin=shoopdaloop=-Wl,-rpath,$ORIGIN/shoop_lib"); // For builtin libraries
    println!("cargo:rustc-link-arg-bin=shoopdaloop=-Wl,-rpath,$ORIGIN/shoop_lib/py/lib"); // For Python library
    println!("cargo:rustc-link-arg-bin=shoopdaloop=-Wl,-rpath,$ORIGIN/shoop_lib/py/{}/PySide6/Qt/lib",
             py_env_to_site_packages.to_str().unwrap()); // For the Qt distribution that comes with PySide6
    println!("cargo:rustc-link-arg-bin=shoopdaloop=-Wl,-rpath,$ORIGIN/lib"); // For bundled dependency libraries

    // Link to dev folders
    println!("cargo:rustc-link-arg-bin=shoopdaloop_dev=-Wl,-rpath,{}/..", py_env_dir.to_str().unwrap()); // For builtin libraries
    println!("cargo:rustc-link-arg-bin=shoopdaloop_dev=-Wl,-rpath,{}/lib", py_env_dir.to_str().unwrap()); // For Python library
    println!("cargo:rustc-link-arg-bin=shoopdaloop_dev=-Wl,-rpath,{}/{}/PySide6/Qt/lib",
        py_env_dir.to_str().unwrap(),
        py_env_to_site_packages.to_str().unwrap()); // For the Qt distribution that comes with PySide6

    // Rebuild if changed
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/shoopdaloop");

    println!("build.rs finished.");

    // println!("cargo:rustc-link-search=native=/home/sander/.qt-installs/6.6.3/gcc_64/lib");
    // println!("cargo:rustc-link-lib=Qt6Core");
    // println!("cargo:rustc-link-lib=Qt6Gui");
    // println!("cargo:rustc-link-lib=Qt6Quick");

    Ok(())
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