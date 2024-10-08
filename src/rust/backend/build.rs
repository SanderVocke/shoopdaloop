use std::env;
use std::path::PathBuf;
use cmake::Config;
use anyhow;

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
        let lib_path =
            PathBuf::from(env::var("LIBSHOOPDALOOP_DIR").unwrap_or(String::from("../../libshoopdaloop_backend")));
        let install_dir = out_dir.join("cmake_install");
        let cmake_backend_dir = "../../backend";
        let profile = std::env::var("PROFILE").unwrap();
        let cmake_output_dir = out_dir.join("cmake_build");

        if !["debug", "release"].contains(&profile.as_str()) {
            return Err(anyhow::anyhow!("Unknown build profile: {}", &profile));
        }

        // Build back-end via CMake and install into our output directory
        println!("Building back-end...");
        let _ = Config::new(cmake_backend_dir)
            .out_dir(&cmake_output_dir)
            .generator("Ninja")
            .configure_arg(format!("-DCMAKE_INSTALL_PREFIX={}",install_dir.to_str().unwrap()))
            .build();

        println!("cargo:rerun-if-changed={}", cmake_backend_dir);
        println!("cargo:rerun-if-changed=src");
        println!("cargo:rerun-if-changed=build.rs");

        println!("cargo:rustc-env=SHOOP_BACKEND_DIR={}", install_dir.to_str().unwrap());

        println!("cargo:rustc-link-search=native={}", lib_path.display());

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
