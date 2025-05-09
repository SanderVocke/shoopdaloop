use std::process::Command;
use std::env;
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use glob::glob;
use copy_dir::copy_dir;
use anyhow;
use anyhow::Context;
use std::fs;
use std::io;
use walkdir::WalkDir;

pub fn copy_dir_merge(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<(), anyhow::Error> {
    let src = src.as_ref();
    let dst = dst.as_ref();

    for entry in WalkDir::new(src) {
        let entry = entry?;
        let rel_path = entry.path().strip_prefix(src)
           .with_context(|| format!("Failed to strip prefix from path"))?;
        let dest_path = dst.join(rel_path);

        if entry.file_type().is_dir() {
            fs::create_dir_all(&dest_path)
               .with_context(|| format!("Failed to create directory for {}", dest_path.display()))?;
        } else {
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)
                  .with_context(|| format!("Failed to create directory for {}", parent.display()))?;
            }
            fs::copy(entry.path(), &dest_path)
               .with_context(|| format!("Failed to copy file for {}", dest_path.display()))?;
        }
    }
    Ok(())
}

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
        let src_dir = env::current_dir()?;
        let host_python = env::var("PYTHON")
                          .unwrap_or("python3".to_string());
        // let pyo3_python = env::var("PYO3_PYTHON")
        //                         .unwrap_or(host_python);
        let vcpkg_installed_dir = env::var("VCPKG_INSTALLED_DIR")
                                 .with_context(|| "VCPKG_INSTALLED_DIR not set, can't build Python environment")?;
        // Find our singular target triplet inside to get the "true" root
        let candidates : Vec<PathBuf> = glob::glob(format!("{}/*/tools/..", vcpkg_installed_dir).as_str())?
            .filter_map(|path| path.ok())
            .collect();
        if candidates.len() == 0 {
            return Err(anyhow::anyhow!("No triplet environment found in vcpkg build!"));
        }
        if candidates.len() > 1 {
            return Err(anyhow::anyhow!("Multiple triplet environments found in vcpkg build!"));
        }
        let vcpkg_installed_dir = candidates[0].clone();
        let python_src_dir = format!("{}/../../python", src_dir.to_str().unwrap());
        let built_backend_dir = backend::backend_build_dir();

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
        // let py_root_dir = Path::new(&out_dir).join("portable_python");

        if py_env_dir.exists() {
            std::fs::remove_dir_all(&py_env_dir)
                .with_context(|| format!("Failed to remove Py env: {:?}", &py_env_dir))?;
        }
        // if !py_root_dir.exists() {
        //     std::fs::create_dir(&py_root_dir)?;
        // }

        // Create a env in OUT_DIR
        // println!("Creating portable Python...");
        // let py_version = Command::new(host_python)
        //                         .args(&["-c",
        //                                 "import sys; print(f'{sys.version_info.major}.{sys.version_info.minor}')"])
        //                         .output()
        //                         .with_context(|| format!("Failed to print python version: sh ${args:?}"))?;
        // let py_version = std::str::from_utf8(&py_version.stdout)?;
        // let py_version = py_version.trim();
        // let mut py_location : String;

        println!("Cloning Python environment from vcpkg build...");
        copy_dir(&vcpkg_installed_dir, &py_env_dir)
              .with_context(|| format!("Failed to copy environment from {:?} to {:?}", &vcpkg_installed_dir, &py_env_dir))?;

        println!("Merging built backend library into environment...");
        copy_dir_merge(&built_backend_dir, &py_env_dir)
              .with_context(|| format!("Failed to merge built backend from {:?} to {:?}", &built_backend_dir, &py_env_dir))?;

        let prefix_path = if is_debug_build { py_env_dir.join("debug") } else { py_env_dir.clone() };
        let env_python = if is_debug_build { prefix_path.join("tools/python3/python3d") } else { prefix_path.join("tools/python3/python3") };

        // Copy filesets into our output lib dir
        let to_copy = ["lua", "qml", "session_schemas", "../resources"];
        for directory in to_copy {
            let src = src_dir.join("../..").join(directory);
            let dst = py_env_dir.join(PathBuf::from(directory).file_name().unwrap());
            copy_dir_merge(&src, &dst)
                .expect(&format!("Failed to merge {} to {}",
                                src.display(),
                                dst.display()));
        }
        // println!("Using pyenv to install {} to {}...", py_version, py_root_dir.to_str().unwrap());
        // let args = &["install", "--skip-existing", py_version];
        // let pyenv = env::var("PYENV").unwrap_or(String::from("pyenv"));
        // let mut install_env : HashMap<String, String> = env::vars().collect();
        // install_env.insert("PYENV_ROOT".to_string(), py_root_dir.to_str().unwrap().to_string());
        // install_env.insert("PYTHON_CONFIGURE_OPTS".to_string(), "--enable-shared".to_string());
        // println!("   {pyenv:?} {args:?}");
        // println!("   with PYENV_ROOT={py_root_dir:?}");
        // println!("   with PYTHON_CONFIGURE_OPTS=--enable-shared");
        // Command::new(&pyenv)
        //         .args(args)
        //         .envs(install_env)
        //         .status()
        //         .with_context(|| format!("Failed to install Python using pyenv: {pyenv:?} {args:?}"))?;

        // println!("Using uv to install {} to {}...", py_version, py_root_dir.to_str().unwrap());
        // let args = &["python", "install", py_version];
        // // let pyenv = env::var("PYENV").unwrap_or(String::from("pyenv"));
        // let mut install_env : HashMap<String, String> = env::vars().collect();
        // install_env.insert("UV_PYTHON_INSTALL_DIR".to_string(), py_root_dir.to_str().unwrap().to_string());
        // // install_env.insert("PYTHON_CONFIGURE_OPTS".to_string(), "--enable-shared".to_string());
        // // println!("   {pyenv:?} {args:?}");
        // println!("   with UV_PYTHON_INSTALL_DIR={py_root_dir:?}");
        // // println!("   with PYTHON_CONFIGURE_OPTS=--enable-shared");
        // Command::new("uv")
        //         .args(args)
        //         .envs(install_env)
        //         .status()
        //         .with_context(|| format!("Failed to install Python using uv: uv {args:?}"))?;

        // let py_location_output = Command::new(&pyenv)
        //                                 .args(&["prefix", &py_version])
        //                                 .env("PYENV_ROOT", &py_root_dir)
        //                                 .output()
        //                                 .with_context(|| "Failed to get python location output using pyenv")?;

        // let py_location_output = Command::new("uv")
        //     .args(&["python", "dir"])
        //     .env("UV_PYTHON_INSTALL_DIR", &py_root_dir)
        //     .output()
        //     .with_context(|| "Failed to get python location output using uv")?;

        // py_location = String::from(std::str::from_utf8(&py_location_output.stdout)?);
        // if !py_location_output.status.success() {
        //     let maybe_root = env::var("PYENV_ROOT");
        //     if maybe_root.is_ok() {
        //         let maybe_root = maybe_root.unwrap();
        //         let root = Path::new(&maybe_root);
        //         let maybe_py_location = root.join("versions").join(py_version);
        //         if maybe_py_location.exists() {
        //             println!("pyenv prefix command failed. Using guessed prefix {maybe_py_location:?}");
        //             py_location = String::from(maybe_py_location.to_str().unwrap());
        //         } else {
        //             return Err(anyhow::anyhow!("Could not find Python location in pyenv root"));
        //         }
        //     } else {
        //         return Err(anyhow::anyhow!("Could not find Python location in pyenv root"));
        //     }
        // }
        // let py_location = py_location.trim();

        // Install ShoopDaLoop wheel in env
        println!("Using installed Python interpreter: {}", env_python.to_str().unwrap());
        println!("Installing shoopdaloop and dependencies into python env...");
        if !Command::new(&env_python)
            .args(
                &["-m", "ensurepip", "--upgrade"]
                // &["-m", "pip", "install", "--break-system-packages", "--no-input", "--upgrade", "pip"]
            )
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()?
            .success()
        {
            return Err(anyhow::anyhow!("Failed to upgrade pip"));
        }
        if !Command::new(&env_python)
            .args(
                &["-m", "pip", "install", "--break-system-packages", "--no-input", "pytest"]
            )
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()?
            .success()
        {
            return Err(anyhow::anyhow!("Failed to install pytest"));
        }
        if !Command::new(&env_python)
            .args(
                &["-m", "pip", "install", "--break-system-packages", "--no-input", "--force-reinstall", wheel.to_str().expect("Could not get wheel path")]
            )
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()?
            .success()
        {
            return Err(anyhow::anyhow!("Failed to install shoopdaloop wheel"));
        }

        // DLL hack needed on Windows (FIXME: still needed?)
        // #[cfg(target_os = "windows")]
        // {
        //     // Copy all files in (py_env_dir)/Lib\site-packages\pywin32_system32
        //     // to (py_env_dir)/Lib\site-packages\win32
        //     let pywin32_system32 = Path::new(&py_env_dir).join("Lib").join("site-packages").join("pywin32_system32");
        //     let win32 = Path::new(&py_env_dir).join("Lib").join("site-packages").join("win32");
        //     if pywin32_system32.exists() {
        //         for entry in std::fs::read_dir(pywin32_system32)? {
        //             let entry = entry?;
        //             let path = entry.path();
            
        //             // Check if the entry is a file
        //             if path.is_file() {
        //                 // Construct the destination path
        //                 let file_name = path.file_name().unwrap();
        //                 let dest_path = win32.join(file_name);
            
        //                 // Copy the file
        //                 std::fs::copy(&path, &dest_path)?;
        //             }
        //         }
        //     } else {
        //         Err(anyhow::anyhow!("pywin32_system32 not found at {:?}", &pywin32_system32))?;
        //     }
        // }

        let delete_dir = |dir : &Path| -> Result<(), anyhow::Error> {
            if dir.exists() {
                println!("Deleting: {:?}", dir);
                std::fs::remove_dir_all(dir)
                   .with_context(|| format!("Failed to delete {:?}", dir))?;
            } else {
                println!("Not removing because nonexistent: {:?}", dir)
            }
            Ok(())
        };

        // Strip unneeded files and directories from the environment
        if is_debug_build {
            // Remove release folders
            for path in ["bin", "doc", "etc", "include", "lib", "libexec", "metatypes", "Qt6", "sbom", "share"]
            {
                delete_dir(py_env_dir.join(path).as_path());
            }
        } else {
            // Remove debug folder
            delete_dir(py_env_dir.join("debug").as_path())?;
        }
        for path in [
            "doc",
            "etc",
            "include",
            "libexec",
            "share",
        ] {
            // Remove subfolders in the correct build type but we don't need
            delete_dir(prefix_path.join(path).as_path());
        }

        // Rebuild if changed
        println!("cargo:rerun-if-changed=build.rs");
        println!("cargo:rerun-if-changed=src");
        println!("cargo:rerun-if-changed={}/shoopdaloop", python_src_dir);
        println!("cargo:rerun-if-changed={}/pyproject.toml", python_src_dir);
        println!("cargo:rerun-if-env-changed=PYTHON");
        println!("cargo:rerun-if-env-changed=PYO3_PYTHON");

        println!("cargo:rustc-env=SHOOP_PY_ENV_DIR={}", py_env_dir.to_str().unwrap());

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