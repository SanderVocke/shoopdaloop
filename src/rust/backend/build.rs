use anyhow::anyhow;
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
    let refilling_pool_cxx_include = std::env::var("DEP_REFILLING_POOL_INCLUDE")?;
    let refilling_pool_cxx_libdir = std::env::var("DEP_REFILLING_POOL_CXX_BRIDGE_LIBDIR")?;
    let refilling_pool_staticlib = std::env::var("CARGO_STATICLIB_FILE_REFILLING_POOL")?;
    let tracing_cxx_bridge_include = std::env::var("DEP_TRACING_CXX_BRIDGE_INCLUDE")?;
    let tracing_cxx_bridge_libdir =
        std::env::var("DEP_TRACING_CXX_BRIDGE_CXX_BRIDGE_LIBDIR")?;
    let tracing_cxx_bridge_staticlib =
        std::env::var("CARGO_STATICLIB_FILE_TRACING_CXX_BRIDGE")?;
    let profile = std::env::var("PROFILE")?;
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let build_time_link_dirs_raw = option_env!("SHOOP_BUILD_TIME_LINK_DIRS").unwrap_or_default();
    let runtime_link_dirs_raw = option_env!("SHOOP_RUNTIME_LINK_DIRS").unwrap_or_default();

    let install_dir = out_dir.join("cmake_install");
    let cmake_backend_dir = "../../backend";
    let cmake_output_dir = out_dir.join("cmake_build");

    if !["debug", "release"].contains(&profile.as_str()) {
        return Err(anyhow!("Unknown build profile: {}", &profile));
    }

    // Build back-end via CMake and install into our output directory
    {
        println!("Building back-end using CMake...");
        let mut cmake_config: Config = Config::new(cmake_backend_dir);
        let cmake_config_mut: &mut Config = cmake_config
            .out_dir(&cmake_output_dir)
            .generator("Ninja")
            .define("CMAKE_INSTALL_PREFIX", install_dir.to_str().unwrap_or(""))
            .define("REFILLING_POOL_CXX_INCLUDE", refilling_pool_cxx_include)
            .define("REFILLING_POOL_CXX_LIBDIR", refilling_pool_cxx_libdir)
            .define("REFILLING_POOL_RUST_LIB", refilling_pool_staticlib)
            .define(
                "TRACING_CXX_BRIDGE_INCLUDE",
                tracing_cxx_bridge_include,
            )
            .define(
                "TRACING_CXX_BRIDGE_LIBDIR",
                tracing_cxx_bridge_libdir,
            )
            .define(
                "TRACING_CXX_BRIDGE_RUST_LIB",
                tracing_cxx_bridge_staticlib,
            )
            .define(
                "ENABLE_COVERAGE",
                if cfg!(feature = "coverage") {
                    "ON"
                } else {
                    "OFF"
                },
            )
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
        install_dir.to_str().unwrap_or("")
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
