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
    let host_python = env::var("PYTHON").unwrap_or(String::from("python"));
    let shoop_lib_dir = out_dir.join("shoop_lib");
    let cmake_backend_dir = "../../backend";
    let python_dir = "../../python";
    println!("Using Python: {}", host_python);

    if shoop_lib_dir.exists() {
        std::fs::remove_dir_all(&shoop_lib_dir)?;
    }

    // Build back-end via CMake and install into our output directory
    println!("Building back-end...");
    let _ = Config::new(cmake_backend_dir)
        .out_dir(out_dir.join("cmake_build"))
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
    Command::new(&host_python)
        .args(
            &["-m", "build",
            "--outdir", out_dir.to_str().expect("Couldn't get out dir"),
            "--wheel",
            python_dir]
        )
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .current_dir(src_dir.clone())
        .status()?;
    let wheel : PathBuf;
    {
        let whl_pattern = format!("{}/*.whl", out_dir.to_str().unwrap());
        let mut whl_glob = glob(&whl_pattern)?;
        wheel = whl_glob.next()
                .expect(format!("No wheel found @ {}", whl_pattern).as_str())?;
        println!("Found built wheel: {}", wheel.to_str().unwrap());
    }
    // Use hash to determine if the wheel changed
    let wheel_hash = file_hash(&wheel).unwrap();
    let prev_wheel_file = out_dir.join("py_wheel_hash");
    let py_env_dir = Path::new(&out_dir).join("shoop_pyenv");
    let py_env_python = py_env_dir.join("bin").join("python");

    let mut rebuild_env = true;
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
            std::fs::remove_dir_all(&py_env_dir)?;
        }

        // Create a env in OUT_DIR
        println!("Creating portable Python...");
        let py_version = Command::new("sh")
                                .args(&["-c", "python3 --version | sed -r 's/.*3\\./3\\./g'"])
                                .output()?;
        let py_version = std::str::from_utf8(&py_version.stdout)?;
        let py_version = py_version.trim();
        println!("Using pyenv to install {}...", py_version);
        Command::new("pyenv")
                .args(&["install", "--skip-existing", py_version])
                .status()?;
        let py_location = Command::new("pyenv")
                                     .args(&["prefix", &py_version])
                                     .output()?;
        let py_location = std::str::from_utf8(&py_location.stdout)?;
        let py_location = py_location.trim();
        println!("Using Python from: {}. Installing to: {}", py_location, py_env_dir.to_str().unwrap());
        copy_dir(&py_location, &py_env_dir)?;

        // Install ShoopDaLoop wheel in env
        println!("Installing into python env...");
        Command::new(&py_env_python)
            .args(
                &["-m", "pip", "install", "--upgrade", "pip"]
            )
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()?;
        Command::new(&py_env_python)
            .args(
                &["-m", "pip", "install", "--force-reinstall", wheel.to_str().expect("Could not get wheel path")]
            )
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()?;

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
                                 full_site_packages.parent().unwrap().file_name().unwrap().to_str().unwrap()))
    }

    // Tell PyO3 where to find our venv Python
    println!("Setting PYO3_PYTHON to {}", py_env_python.to_str().unwrap());
    env::set_var("PYO3_PYTHON", py_env_python.to_str().unwrap());

    // Set RPATH
    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/shoop_lib"); // For builtin libraries
    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/shoop_lib/py/lib"); // For Python library
    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/shoop_lib/py/{}/PySide6/Qt/lib",
             py_env_to_site_packages.to_str().unwrap()); // For the Qt distribution that comes with PySide6
    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/lib"); // For bundled dependency libraries

    // Rebuild if changed
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/shoopdaloop");

    println!("build.rs finished.");

    println!("cargo:rustc-link-search=native=/home/sander/.qt-installs/6.6.3/gcc_64/lib");
    println!("cargo:rustc-link-lib=Qt6Core");
    println!("cargo:rustc-link-lib=Qt6Gui");
    println!("cargo:rustc-link-lib=Qt6Quick");

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