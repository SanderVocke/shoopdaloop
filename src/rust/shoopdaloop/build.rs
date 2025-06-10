use std::env;
use std::path::PathBuf;
use glob::glob;
use copy_dir::copy_dir;
use anyhow;
use anyhow::Context;
use backend;
use py_env;
use common;
use config;

fn main_impl() -> Result<(), anyhow::Error> {
    // If we're pre-building, don't do anything
    #[cfg(feature = "prebuild")]
    {
        println!("cargo:rustc-env=SHOOP_RUNTIME_LINK_PATHS=");
        return Ok(());
    }

    #[cfg(not(feature = "prebuild"))]
    {
        let src_dir = env::current_dir()?;
        
        let dev_env_python = py_env::dev_env_python();
        let dev_venv_dir = py_env::dev_venv_dir();

        let profile = std::env::var("PROFILE").unwrap();
        let is_debug_build = std::env::var("PROFILE").unwrap() == "debug";
        
        // let env_lib_dir = if is_debug_build { dev_venv_dir.join("debug/lib") } else { dev_venv_dir.join("lib") };

        if !["debug", "release"].contains(&profile.as_str()) {
            return Err(anyhow::anyhow!("Unknown build profile: {}", &profile));
        }

        println!("Using dev env Python: {}", dev_env_python);

        // Make env dir information available
        println!("cargo:rustc-env=SHOOP_DEV_VENV_DIR={}", dev_venv_dir.to_str().unwrap());
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

        #[cfg(target_os = "linux")] {
            // Link to portable lib folder
            println!("cargo:rustc-link-arg-bin=shoopdaloop=-Wl,-rpath,$ORIGIN/../lib");

            // Use RPATH instead of RUNPATH, which will enable finding transitive dependencies
            // (e.g. in the vcpkg installation folder)
            println!("cargo:rustc-link-arg-bin=shoopdaloop=-Wl,--disable-new-dtags");
        }

        for path in backend::build_time_link_dirs() {
            println!("cargo:rustc-link-search=native={:?}", path);
        }

        let backend_runtime_link_paths_str = 
           backend::runtime_link_dirs()
              .iter()
              .map(|p| p.to_string_lossy().to_string())
              .collect::<Vec<String>>()
              .join(common::fs::PATH_LIST_SEPARATOR);
        println!("cargo:rustc-env=SHOOP_RUNTIME_LINK_PATHS={}", backend_runtime_link_paths_str);

        // Rebuild if changed
        println!("cargo:rerun-if-changed=build.rs");
        println!("cargo:rerun-if-changed=src");

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