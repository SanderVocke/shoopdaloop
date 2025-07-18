use anyhow;
use backend;
use common;
use std::path::PathBuf;

fn generate_dev_launcher_script() -> Result<PathBuf, anyhow::Error> {
    #[cfg(feature = "prebuild")]
    {
        return Ok(PathBuf::default());
    }

    #[cfg(not(feature = "prebuild"))]
    {
        use config::{self, config::ShoopConfig};
        use std::io::Write;

        let dev_config_path = config::dev_config_path();
        let dev_config_path_str = dev_config_path.to_string_lossy().replace("\"", "");

        let dev_config =
            ShoopConfig::_load(&dev_config_path, None).expect("Could not load dev config");

        let (_var, paths) = config::config_dynlib_env_var(&dev_config)?;

        if cfg!(target_os = "windows") {
            let script_content = format!(
                r#"
@echo off
SET "PATH=%PATH%;{paths}"
SET "SHOOP_CONFIG={dev_config_path_str}"
%SHOOP_CMD_PREFIX% "%~dp0shoopdaloop.exe" %*
"#
            );
            let script_path = PathBuf::from(std::env::var("OUT_DIR").unwrap())
                .ancestors()
                .nth(3)
                .unwrap()
                .join("shoopdaloop_dev.bat");
            let mut file = std::fs::File::create(&script_path)?;
            file.write_all(script_content.as_bytes())?;
            file.sync_all()?;
            return Ok(script_path);
        } else if cfg!(target_os = "linux") {
            let script_content = format!(
                r#"
#!/bin/sh
SCRIPT_DIR=$(dirname "$(readlink -f "$0")")
export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:{paths}"
export SHOOP_CONFIG="{dev_config_path_str}"
$SCRIPT_DIR//shoopdaloop "$@"
"#
            );
            let script_path = PathBuf::from(std::env::var("OUT_DIR").unwrap())
                .ancestors()
                .nth(3)
                .unwrap()
                .join("shoopdaloop_dev.sh");
            let mut file = std::fs::File::create(&script_path)?;
            file.write_all(script_content.as_bytes())?;
            file.sync_all()?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = std::fs::metadata(&script_path)?.permissions();
                perms.set_mode(0o755); // rwxr-xr-x
                std::fs::set_permissions(&script_path, perms)?;
            }

            return Ok(script_path);
        }
        return Ok(PathBuf::default());
    }
}

fn main_impl() -> Result<(), anyhow::Error> {
    // If we're pre-building, don't do anything
    if cfg!(feature = "prebuild") {
        println!("cargo:rustc-env=SHOOP_RUNTIME_LINK_PATHS=");
        return Ok(());
    }

    println!("Creating dev launcher script");
    let dev_launcher_script = generate_dev_launcher_script()?;
    println!("cargo:rustc-env=SHOOPDALOOP_DEV_LAUNCHER_SCRIPT={dev_launcher_script:?}");

    let profile = std::env::var("PROFILE").unwrap();
    if !["debug", "release", "release-with-debug"].contains(&profile.as_str()) {
        return Err(anyhow::anyhow!("Unknown build profile: {}", &profile));
    }

    #[cfg(target_os = "linux")]
    {
        println!("cargo:rustc-link-arg-bin=shoopdaloop=-Wl,--no-as-needed");
    }
    println!("cargo:rustc-link-arg-bin=shoopdaloop=-lshoopdaloop_backend");
    #[cfg(target_os = "windows")]
    {
        // force linkage by manually importing an arbitrary symbol
        println!("cargo:rustc-link-arg-bin=shoopdaloop=/INCLUDE:create_audio_driver");
        println!("cargo:rustc-link-lib=shoopdaloop_backend");
    }

    #[cfg(target_os = "linux")]
    {
        // Link to portable lib folder
        println!("cargo:rustc-link-arg-bin=shoopdaloop=-Wl,-rpath,$ORIGIN/../lib");

        // Use RPATH instead of RUNPATH, which will enable finding transitive dependencies
        // (e.g. in the vcpkg installation folder)
        println!("cargo:rustc-link-arg-bin=shoopdaloop=-Wl,--disable-new-dtags");
    }

    for path in backend::build_time_link_dirs() {
        println!("cargo:rustc-link-search=native={:?}", path);
    }

    let backend_runtime_link_paths_str = backend::runtime_link_dirs()
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<String>>()
        .join(common::util::PATH_LIST_SEPARATOR);
    println!(
        "cargo:rustc-env=SHOOP_RUNTIME_LINK_PATHS={}",
        backend_runtime_link_paths_str
    );

    // Rebuild if changed
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src");

    println!("build.rs finished.");
    Ok(())
}

fn main() {
    match main_impl() {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Error: {:?}\nBacktrace: {:?}", e, e.backtrace());
            std::process::exit(1);
        }
    }
}
