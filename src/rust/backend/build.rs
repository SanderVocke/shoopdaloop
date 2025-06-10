use std::env;
use std::path::PathBuf;
use cmake::Config;
use anyhow;
use common;

// For now, Rust "back-end" is just a set of C bindings to the
// C++ back-end.
fn main_impl() -> Result<(), anyhow::Error> {
    // If we're pre-building, don't do anything
    #[cfg(feature = "prebuild")]
    {
        return Ok(());
    }

    #[cfg(not(feature = "prebuild"))]
    {
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let install_dir = out_dir.join("cmake_install");
        let cmake_backend_dir = "../../backend";
        let profile = std::env::var("PROFILE").unwrap();
        let cmake_output_dir = out_dir.join("cmake_build");

        if !["debug", "release"].contains(&profile.as_str()) {
            return Err(anyhow::anyhow!("Unknown build profile: {}", &profile));
        }

        // Build back-end via CMake and install into our output directory
        {
            println!("Building back-end using CMake...");
            let mut cmake_config: Config = Config::new(cmake_backend_dir);
            let cmake_config_mut : &mut Config = cmake_config.out_dir(&cmake_output_dir)
                                            .generator("Ninja")
                                            .configure_arg(format!("-DCMAKE_INSTALL_PREFIX={}",install_dir.to_str().unwrap()))
                                            .configure_arg(format!("-DCMAKE_BUILD_TYPE={}", if profile == "debug" { "Debug" } else { "Release" }));
            let _ = cmake_config_mut.build();
        }

        let build_time_link_dirs_raw = option_env!("SHOOP_BUILD_TIME_LINK_DIRS").unwrap_or_default();
        let runtime_link_dirs_raw = option_env!("SHOOP_RUNTIME_LINK_DIRS").unwrap_or_default();

        let build_time_link_dirs = build_time_link_dirs_raw
            .split(common::fs::PATH_LIST_SEPARATOR)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .collect::<Vec<_>>();

        println!("cargo:rerun-if-changed={}", cmake_backend_dir);
        println!("cargo:rerun-if-changed=src");
        println!("cargo:rerun-if-changed=build.rs");
        println!("cargo:rerun-if-env-changed=PYTHON");

        println!(
            "cargo:rustc-env=SHOOP_BUILD_TIME_LINK_DIRS={}",
            build_time_link_dirs_raw
        );
        println!(
            "cargo:rustc-env=SHOOP_RUNTIME_LINK_DIRS={}",
            runtime_link_dirs_raw
        );
        println!("cargo:rustc-env=SHOOP_BACKEND_DIR={}", install_dir.to_str().unwrap());
        for path in build_time_link_dirs.iter() {
            println!("cargo:rustc-link-search=native={}", path.display());
        }

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
