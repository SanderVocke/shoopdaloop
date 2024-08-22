use std::process::Command;
use std::env;
use std::path::{Path, PathBuf};
use glob::glob;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let src_dir = env::current_dir().unwrap();
    let host_python = env::var("PYTHON").unwrap_or(String::from("python"));

    // Build ShoopDaLoop wheel
    let skip_rebuild = env::var("SHOOP_SKIP_REBUILD").is_ok();
    if !skip_rebuild {
        Command::new(&host_python)
            .args(
                &["-m", "build", "--wheel", src_dir.to_str().expect("Couldn't get source dir")]
            )
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .current_dir(src_dir.clone())
            .status().unwrap();
    }
    let wheel : PathBuf;
    {
        let whl_pattern = format!("{}/dist/*.whl", src_dir.to_str().unwrap());
        let mut whl_glob = glob(&whl_pattern).unwrap();
        wheel = whl_glob.next()
                .expect(format!("No wheel found @ {}", whl_pattern).as_str())
                .unwrap();
        println!("Found built wheel: {}", wheel.to_str().unwrap());
    }

    // Create a venv in OUT_DIR
    let venv_dir = Path::new(&out_dir).join("build_venv");
    Command::new(&host_python).args(
        &["-m", "venv", venv_dir.to_str().expect("Could not get venv path")]
    ).status().unwrap();
    let venv_python = venv_dir.join("bin").join("python");

    // Install ShoopDaLoop wheel in venv
    Command::new(&venv_python)
        .args(
            &["-m", "pip", "install", wheel.to_str().expect("Could not get wheel path")]
        )
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status().unwrap();
    println!("Installed ShoopDaLoop wheel in venv");

    // Tell PyO3 where to find our venv Python
    println!("Setting PYO3_PYTHON to {}", venv_python.to_str().unwrap());
    env::set_var("PYO3_PYTHON", venv_python.to_str().unwrap());

    // println!("cargo::rustc-link-search=native={}", out_dir);
    // println!("cargo::rustc-link-lib=static=hello");
    // println!("cargo::rerun-if-changed=src/hello.c");
}