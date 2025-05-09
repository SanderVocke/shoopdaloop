use std::env;
use std::path::PathBuf;
use glob::glob;
use copy_dir::copy_dir;
use anyhow;
use anyhow::Context;
// use backend;
use py_env;

fn main_impl() -> Result<(), anyhow::Error> {
    // If we're pre-building, don't do anything
    #[cfg(feature = "prebuild")]
    {
        return Ok(());
    }

    #[cfg(not(feature = "prebuild"))]
    {
        let src_dir = env::current_dir()?;
        let host_python = env::var("PYTHON").unwrap_or(String::from("python3"));
        let profile = std::env::var("PROFILE").unwrap();
        let py_env_dir = py_env::py_env_dir();
        let is_debug_build = std::env::var("PROFILE").unwrap() == "debug";
        let env_lib_dir = if is_debug_build { py_env_dir.join("debug/lib") } else { py_env_dir.join("lib") };

        if !["debug", "release"].contains(&profile.as_str()) {
            return Err(anyhow::anyhow!("Unknown build profile: {}", &profile));
        }

        println!("Using Python: {}", host_python);

        // if shoop_lib_dir.exists() {
        //     std::fs::remove_dir_all(&shoop_lib_dir)
        //         .with_context(|| format!("Failed to remove {:?}", &shoop_lib_dir))?;
        // }

        // // Copy the base built backend
        // copy_dir(&built_backend_dir, &shoop_lib_dir)?;

        // // Copy the base python directory
        // copy_dir(&py_env_dir, shoop_lib_dir.join("py"))?;

        let path_to_runtime_lib = if is_debug_build {
            "runtime/debug/lib"
        } else {
            "runtime/lib"
        };
        let path_to_site_packages : PathBuf;
        {
            let pattern = format!("{}/**/site-packages", env_lib_dir.to_str().unwrap());
            let mut sp_glob = glob(&pattern).expect("Couldn't glob for site-packages");
            let full_site_packages = sp_glob.next()
                    .expect(format!("No site-packages dir found @ {}", pattern).as_str()).unwrap();
            path_to_site_packages = full_site_packages.strip_prefix(&py_env_dir).unwrap().to_path_buf();
        }

        // Make env dir information available
        println!("cargo:rustc-env=SHOOP_RUNTIME_ENV_DIR={}", py_env_dir.to_str().unwrap());
        println!("cargo:rustc-env=SHOOP_ENV_DYLIB_DIR={}", env_lib_dir.to_str().unwrap());
        println!("cargo:rustc-env=SHOOP_ENV_DIR_TO_SITE_PACKAGES={}", path_to_site_packages.to_str().unwrap());
        println!("cargo:rustc-env=SHOOP_ENV_DIR_TO_RUNTIME_LIB={}", path_to_runtime_lib);

        // Link to libshoopdaloop_backend
        println!("cargo:rustc-link-search=native={}", env_lib_dir.to_str().unwrap());
        #[cfg(target_os = "linux")]
        {
            println!("cargo:rustc-link-arg-bin=shoopdaloop_dev=-Wl,--no-as-needed");
        }
        println!("cargo:rustc-link-arg-bin=shoopdaloop_dev=-lshoopdaloop_backend");
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

        // Configure RPATHs 
        #[cfg(target_os = "linux")]
        {
            // Set RPATH
            println!("cargo:rustc-link-arg-bin=shoopdaloop=-Wl,-rpath,$ORIGIN/{}", path_to_runtime_lib); // For builtin libraries
        }
        #[cfg(target_os = "macos")]
        {
            // Set RPATH
            println!("cargo:rustc-link-arg-bin=shoopdaloop=-Wl,-rpath,$loader_path/{}", path_to_runtime_lib); // For builtin libraries
        }

        // Link to dev folders
        println!("cargo:rustc-link-arg-bin=shoopdaloop_dev=-Wl,-rpath,{}", env_lib_dir.to_str().unwrap());

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