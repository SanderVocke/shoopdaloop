use std::process::Command;
use std::env;
use std::path::{Path, PathBuf};
use glob::glob;
use cmake::Config;
use copy_dir::copy_dir;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let src_dir = env::current_dir().unwrap();
    let host_python = env::var("PYTHON").unwrap_or(String::from("python"));
    let shoop_lib_dir = out_dir.join("shoop_lib");
    let cmake_backend_dir = "../../backend";
    let python_dir = "../../python";
    println!("Using Python: {}", host_python);

    let _ = std::fs::remove_dir_all(&shoop_lib_dir);

    // Build back-end via CMake and install into our output directory
    println!("Building back-end...");
    let _ = Config::new(cmake_backend_dir)
        .out_dir(out_dir.join("cmake_build"))
        .configure_arg(format!("-DCMAKE_INSTALL_PREFIX={}",shoop_lib_dir.to_str().unwrap()))
        .build();

    // Copy filesets into our output lib dir
    let to_copy = ["lua", "qml", "session_schemas"];
    for directory in to_copy {
        let src = src_dir.join("../..").join(directory);
        let dst = shoop_lib_dir.join(directory);
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
        .status().unwrap();
    let wheel : PathBuf;
    {
        let whl_pattern = format!("{}/*.whl", out_dir.to_str().unwrap());
        let mut whl_glob = glob(&whl_pattern).unwrap();
        wheel = whl_glob.next()
                .expect(format!("No wheel found @ {}", whl_pattern).as_str())
                .unwrap();
        println!("Found built wheel: {}", wheel.to_str().unwrap());
    }

    // Create a venv in OUT_DIR
    println!("Creating venv...");
    let venv_dir = Path::new(&out_dir).join("shoop_pyenv");
    Command::new(&host_python).args(
        &["-m", "venv", venv_dir.to_str().expect("Could not get venv path")]
    ).status().unwrap();
    let venv_python = venv_dir.join("bin").join("python");

    // Install ShoopDaLoop wheel in venv
    println!("Installing into venv...");
    Command::new(&venv_python)
        .args(
            &["-m", "pip", "install", "--force-reinstall", wheel.to_str().expect("Could not get wheel path")]
        )
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status().unwrap();
    let site_packages_dir : PathBuf;
    {
        let pattern = format!("{}/**/site-packages", venv_dir.to_str().unwrap());
        let mut sp_glob = glob(&pattern).unwrap();
        site_packages_dir = sp_glob.next()
                .expect(format!("No site-packages dir found @ {}", pattern).as_str())
                .unwrap();
    }
    let shoop_package_dir = site_packages_dir.join("shoopdaloop");

    // Copy additional generated Python code in
    std::fs::copy(
        shoop_lib_dir.join("libshoopdaloop_bindings.py"),
        &shoop_package_dir.join("libshoopdaloop_bindings.py")
    ).unwrap();
    std::fs::copy(
        shoop_lib_dir.join("shoop_crashhandling.py"),
        &shoop_package_dir.join("shoop_crashhandling.py")
    ).unwrap();

    // Tell PyO3 where to find our venv Python
    println!("Setting PYO3_PYTHON to {}", venv_python.to_str().unwrap());
    env::set_var("PYO3_PYTHON", venv_python.to_str().unwrap());

    // Set RPATH
    println!("cargo:rustc-link-arg=-Wl,-rpath,./shoop_lib");

    // Rebuild if changed
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/shoopdaloop");

    println!("build.rs finished.");

    println!("cargo:rustc-link-search=native=/home/sander/.qt-installs/6.6.3/gcc_64/lib");
    println!("cargo:rustc-link-lib=Qt6Core");
    println!("cargo:rustc-link-lib=Qt6Gui");
    println!("cargo:rustc-link-lib=Qt6Quick");
}