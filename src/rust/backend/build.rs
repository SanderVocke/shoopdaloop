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

        // FIXME: remove if already builtin
        // Use CMake toolchain file from env if set
        // let toolchain_file : Option<String>;
        // {
        //     let maybe_toolchain_file = env::var("CMAKE_TOOLCHAIN_FILE");
        //     toolchain_file = if maybe_toolchain_file.is_ok() {
        //         let toolchain_file_str : String = maybe_toolchain_file.unwrap();
        //         println!("using CMAKE_TOOLCHAIN_FILE: {}", toolchain_file_str);
        //         Some(toolchain_file_str)
        //     } else {
        //         println!("Note: No CMAKE_TOOLCHAIN_FILE override set");
        //         None
        //     }
        // } 


        if !["debug", "release"].contains(&profile.as_str()) {
            return Err(anyhow::anyhow!("Unknown build profile: {}", &profile));
        }

        // Build back-end via CMake and install into our output directory
        {
            println!("Building back-end using CMake...");
            let mut cmake_config: Config = Config::new(cmake_backend_dir);
            let cmake_config_mut : &mut Config = cmake_config.out_dir(&cmake_output_dir)
                                            .generator("Ninja")
                                            .configure_arg(format!("-DCMAKE_INSTALL_PREFIX={}",install_dir.to_str().unwrap()));
            // if toolchain_file.is_some() {
            //     cmake_config_mut = cmake_config_mut.configure_arg(format!("-DCMAKE_TOOLCHAIN_FILE={}", toolchain_file.unwrap()));
            // }
            let _ = cmake_config_mut.build();
        }

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
