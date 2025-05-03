use std::env;
use std::path::PathBuf;
use glob::glob;
use copy_dir::copy_dir;
use anyhow;
use anyhow::Context;
use backend;
use py_env;

fn main_impl() -> Result<(), anyhow::Error> {
    // If we're pre-building, don't do anything
    #[cfg(feature = "prebuild")]
    {
        return Ok(());
    }

    #[cfg(not(feature = "prebuild"))]
    {
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let src_dir = env::current_dir()?;
        let host_python = env::var("PYTHON").unwrap_or(String::from("python3"));
        let shoop_lib_dir = out_dir.join("shoop_lib");
        let profile = std::env::var("PROFILE").unwrap();
        let built_backend_dir = backend::backend_build_dir();
        let py_env_dir = py_env::py_env_dir();
        let py_interpreter = py_env::py_interpreter();

        if !["debug", "release"].contains(&profile.as_str()) {
            return Err(anyhow::anyhow!("Unknown build profile: {}", &profile));
        }

        println!("Using Python: {}", host_python);

        if shoop_lib_dir.exists() {
            std::fs::remove_dir_all(&shoop_lib_dir)
                .with_context(|| format!("Failed to remove {:?}", &shoop_lib_dir))?;
        }

        // Copy the base built backend
        copy_dir(&built_backend_dir, &shoop_lib_dir)?;

        // Copy the base python directory
        copy_dir(&py_env_dir, shoop_lib_dir.join("py"))?;

        // Copy filesets into our output lib dir
        let to_copy = ["lua", "qml", "session_schemas", "../resources"];
        for directory in to_copy {
            let src = src_dir.join("../..").join(directory);
            let dst = shoop_lib_dir.join(PathBuf::from(directory).file_name().unwrap());
            copy_dir(&src, &dst)
                .expect(&format!("Failed to copy {} to {}",
                                src.display(),
                                dst.display()));
        }

        let py_env_to_site_packages : PathBuf;
        {
            let pattern = format!("{}/**/site-packages", py_env_dir.to_str().unwrap());
            let mut sp_glob = glob(&pattern).expect("Couldn't glob for site-packages");
            let full_site_packages = sp_glob.next()
                    .expect(format!("No site-packages dir found @ {}", pattern).as_str()).unwrap();
            py_env_to_site_packages = PathBuf::from
                                    (format!("lib/{}/site-packages",
                                    full_site_packages.parent().unwrap().file_name().unwrap().to_str().unwrap()));
        }

        // Link to libshoopdaloop_backend
        println!("cargo:rustc-link-search=native={}", shoop_lib_dir.to_str().unwrap());
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
            // force linkage by manually importing a symbol
            println!("cargo:rustc-link-arg-bin=shoopdaloop=/INCLUDE:create_audio_driver");
            println!("cargo:rustc-link-lib=shoopdaloop_backend");
        }

        #[cfg(target_os = "linux")]
        {
            // Set RPATH
            println!("cargo:rustc-link-arg-bin=shoopdaloop=-Wl,-rpath,$ORIGIN/shoop_lib"); // For builtin libraries
            println!("cargo:rustc-link-arg-bin=shoopdaloop=-Wl,-rpath,$ORIGIN/shoop_lib/py/lib"); // For Python library
            println!("cargo:rustc-link-arg-bin=shoopdaloop=-Wl,-rpath,$ORIGIN/shoop_lib/py/{}/PySide6/Qt/lib",
                    py_env_to_site_packages.to_str().unwrap()); // For the Qt distribution that comes with PySide6
            println!("cargo:rustc-link-arg-bin=shoopdaloop=-Wl,-rpath,$ORIGIN/lib"); // For bundled dependency libraries
        }

        #[cfg(target_os = "macos")]
        {
            // Set RPATH
            println!("cargo:rustc-link-arg-bin=shoopdaloop=-Wl,-rpath,@loader_path/shoop_lib"); // For builtin libraries
            println!("cargo:rustc-link-arg-bin=shoopdaloop=-Wl,-rpath,@loader_path/shoop_lib/py/lib"); // For Python library
            println!("cargo:rustc-link-arg-bin=shoopdaloop=-Wl,-rpath,@loader_path/shoop_lib/py/{}/PySide6/Qt/lib",
                    py_env_to_site_packages.to_str().unwrap()); // For the Qt distribution that comes with PySide6
            println!("cargo:rustc-link-arg-bin=shoopdaloop=-Wl,-rpath,@loader_path/lib"); // For bundled dependency libraries
        }

        // Link to dev folders
        println!("cargo:rustc-link-arg-bin=shoopdaloop_dev=-Wl,-rpath,{}/..", py_env_dir.to_str().unwrap());
        println!("cargo:rustc-link-arg-bin=shoopdaloop_dev=-Wl,-rpath,{}/lib", py_env_dir.to_str().unwrap());
        println!("cargo:rustc-link-arg-bin=shoopdaloop_dev=-Wl,-rpath,{}/{}/PySide6/Qt/lib",
            py_env_dir.to_str().unwrap(),
            py_env_to_site_packages.to_str().unwrap()); // For the Qt distribution that comes with PySide6
        println!("cargo:rustc-link-arg-bin=shoopdaloop_dev=-Wl,-rpath,{}", shoop_lib_dir.to_str().unwrap());

        // The build env may pass additional RPATHs for the dev executable to work (e.g. for vcpkg dependencies)
        let maybe_extra_path = std::env::var("SHOOPDALOOP_DEV_EXTRA_DYLIB_PATH");
        if maybe_extra_path.is_ok() {
            println!("cargo:rustc-link-arg-bin=shoopdaloop_dev=-Wl,-rpath,{}", maybe_extra_path.unwrap());
        }

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