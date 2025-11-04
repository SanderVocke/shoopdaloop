use anyhow;
use cmake::Config;
use common;
use std::env;
use std::path::PathBuf;

// For now, Rust "back-end" is just a set of C bindings to the
// C++ back-end.
fn main_impl() -> Result<(), anyhow::Error> {
    // If we're pre-building, don't do anything
    if cfg!(feature = "prebuild") {
        return Ok(());
    }

    // environment
    let refilling_pool_cxx_include = std::env::var("DEP_REFILLING_POOL_INCLUDE").unwrap();
    let refilling_pool_cxx_libdir = std::env::var("DEP_REFILLING_POOL_CXX_BRIDGE_LIBDIR").unwrap();
    let refilling_pool_staticlib = std::env::var("CARGO_STATICLIB_FILE_REFILLING_POOL").unwrap();
    let profile = std::env::var("PROFILE").unwrap();
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let build_time_link_dirs_raw = option_env!("SHOOP_BUILD_TIME_LINK_DIRS").unwrap_or_default();
    let runtime_link_dirs_raw = option_env!("SHOOP_RUNTIME_LINK_DIRS").unwrap_or_default();

    let install_dir = out_dir.join("cmake_install");
    let cmake_backend_dir = "../../backend";
    let cmake_output_dir = out_dir.join("cmake_build");

    if !["debug", "release"].contains(&profile.as_str()) {
        return Err(anyhow::anyhow!("Unknown build profile: {}", &profile));
    }

    // Build back-end via CMake and install into our output directory
    {
        println!("Building back-end using CMake...");
        let mut cmake_config: Config = Config::new(cmake_backend_dir);
        let cmake_config_mut: &mut Config = cmake_config
            .out_dir(&cmake_output_dir)
            .generator("Ninja")
            .define("CMAKE_INSTALL_PREFIX", install_dir.to_str().unwrap())
            .define("REFILLING_POOL_CXX_INCLUDE", refilling_pool_cxx_include)
            .define("REFILLING_POOL_CXX_LIBDIR", refilling_pool_cxx_libdir)
            .define("REFILLING_POOL_RUST_LIB", refilling_pool_staticlib)
            .define(
                "CMAKE_BUILD_TYPE",
                if profile == "debug" {
                    "Debug"
                } else if profile == "release-with-debug" {
                    "RelWithDebInfo"
                } else {
                    "Release"
                },
            );
        let _ = cmake_config_mut.build();
    }

    let build_time_link_dirs = build_time_link_dirs_raw
        .split(common::util::PATH_LIST_SEPARATOR)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect::<Vec<_>>();

    println!("cargo:rerun-if-changed={}", cmake_backend_dir);
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=build.rs");

    println!(
        "cargo:rustc-env=SHOOP_BUILD_TIME_LINK_DIRS={}",
        build_time_link_dirs_raw
    );
    println!(
        "cargo:rustc-env=SHOOP_RUNTIME_LINK_DIRS={}",
        runtime_link_dirs_raw
    );
    println!(
        "cargo:rustc-env=SHOOP_BACKEND_DIR={}",
        install_dir.to_str().unwrap()
    );
    for path in build_time_link_dirs.iter() {
        println!("cargo:rustc-link-search=native={}", path.display());
    }

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
