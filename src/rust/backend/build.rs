use std::env;
use std::path::PathBuf;
use cmake::Config;
use anyhow;
use glob::glob;
use anyhow::Context;

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
        {
            println!("Building back-end using CMake...");
            let mut cmake_config: Config = Config::new(cmake_backend_dir);
            let cmake_config_mut : &mut Config = cmake_config.out_dir(&cmake_output_dir)
                                            .generator("Ninja")
                                            .configure_arg(format!("-DCMAKE_INSTALL_PREFIX={}",install_dir.to_str().unwrap()))
                                            .configure_arg(format!("-DCMAKE_BUILD_TYPE={}", if profile == "debug" { "Debug" } else { "Release" }));
            let _ = cmake_config_mut.build();
        }
        let zita_resampler_dylib_dir_txtfile : PathBuf
            = cmake_output_dir.join("build/zita_resampler_dir.txt");
        // Read the contents
        let zita_resampler_dylib_dir = std::fs::read_to_string(&zita_resampler_dylib_dir_txtfile)
            .with_context(|| format!("Failed to read {:?}", &zita_resampler_dylib_dir_txtfile))?;

        // {
        //     let pattern = format!("{}/**/site-packages", env_lib_dir.to_str().unwrap());
        //     let mut sp_glob = glob(&pattern).expect("Couldn't glob for site-packages");
        //     let full_site_packages = sp_glob.next()
        //             .expect(format!("No site-packages dir found @ {}", pattern).as_str()).unwrap();
        //     path_to_site_packages = full_site_packages.strip_prefix(&dev_venv_dir).unwrap().to_path_buf();
        // }

        println!("cargo:rerun-if-changed={}", cmake_backend_dir);
        println!("cargo:rerun-if-changed=src");
        println!("cargo:rerun-if-changed=build.rs");
        println!("cargo:rerun-if-env-changed=PYTHON");

        println!("cargo:rustc-env=SHOOP_ZITA_LINK_DIR={}", zita_resampler_dylib_dir);
        println!("cargo:rustc-env=SHOOP_BACKEND_DIR={}", install_dir.to_str().unwrap());
        
        println!("cargo:rustc-link-search=native={}", lib_path.display());
        println!("cargo:rustc-link-search=native={}", zita_resampler_dylib_dir);

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
