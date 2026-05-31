use anyhow::anyhow;
use cmake::Config;
use common;
use std::env;
use std::path::{Path, PathBuf};

fn short_cmake_output_dir(out_dir: &Path, profile: &str) -> PathBuf {
    // Cargo's OUT_DIR is deeply nested, e.g.
    // target/<profile>/build/backend-<hash>/out. Corrosion places its Cargo
    // target dir under the CMake binary dir, and cxx-build then appends more
    // generated path components. On Windows release-with-debug builds this can
    // exceed MAX_PATH for long bridge names. Keep the CMake binary dir near the
    // Cargo profile dir while preserving uniqueness per backend build-script hash.
    let build_hash = out_dir
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or("backend")
        .trim_start_matches("backend-");
    let short_hash = &build_hash[..build_hash.len().min(8)];

    if let Some(target_profile_dir) = out_dir
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
    {
        target_profile_dir.join(format!("b-{short_hash}"))
    } else {
        PathBuf::from("target")
            .join(profile)
            .join(format!("b-{short_hash}"))
    }
}

// For now, Rust "back-end" is just a set of C bindings to the
// C++ back-end.
fn main_impl() -> Result<(), anyhow::Error> {
    // If we're pre-building, don't do anything
    if cfg!(feature = "prebuild") {
        return Ok(());
    }

    // environment
    let profile = std::env::var("PROFILE")?;
    let rustflags = std::env::var("RUSTFLAGS").unwrap_or_default();
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let build_time_link_dirs_raw = option_env!("SHOOP_BUILD_TIME_LINK_DIRS").unwrap_or_default();
    let runtime_link_dirs_raw = option_env!("SHOOP_RUNTIME_LINK_DIRS").unwrap_or_default();

    let install_dir = out_dir.join("cmake_install");
    let cmake_backend_dir = "../../backend";
    let cmake_output_dir = short_cmake_output_dir(&out_dir, &profile);

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
            .define("SHOOP_USE_CORROSION_BACKEND_RUST", "ON")
            .define("SHOOP_CORROSION_RUSTFLAGS", rustflags)
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
            )
            // Force cmake rebuild
            .define("SHOOP_FORCE_REBUILD", "v5");
        let _ = cmake_config_mut.build();
    }

    let build_time_link_dirs = build_time_link_dirs_raw
        .split(common::util::PATH_LIST_SEPARATOR)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect::<Vec<_>>();

    println!("cargo:rerun-if-changed={}", cmake_backend_dir);
    println!(
        "cargo:rerun-if-changed={}/CMakeLists.txt",
        cmake_backend_dir
    );
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
