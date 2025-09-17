use anyhow;
use backend;

// For now, Rust "back-end" is just a set of C bindings to the
// C++ back-end.
fn main_impl() -> Result<(), anyhow::Error> {
    let profile = std::env::var("PROFILE").unwrap();
    let bindings_header_path = "../../backend/libshoopdaloop_backend.h";
    let lib_path = if profile == "debug" {
        backend::backend_build_dir().join("debug/lib")
    } else {
        backend::backend_build_dir().join("lib")
    };
    let gen_lib_path = "src/codegen/libshoopdaloop_backend.rs";

    // Generate Rust bindings
    let bindings = bindgen::Builder::default()
        .header(bindings_header_path)
        .generate()
        .expect("Unable to generate Rust bindings for libshoopdaloop_backend.");

    bindings
        .write_to_file(gen_lib_path)
        .expect("Couldn't write Rust libshoopdaloop_backend bindings.");

    println!("cargo:rerun-if-changed={}", bindings_header_path);
    println!("cargo:rerun-if-changed=src/lib.rs");

    println!("cargo:rustc-link-search=native={}", lib_path.display());
    for path in backend::build_time_link_dirs() {
        println!("cargo:rustc-link-search=native={}", path.display());
    }

    // FIXME: Even though this is a transitive dependency it is needed here
    // to prevent linking errors. Why?
    println!("cargo:rustc-link-lib=dylib=zita-resampler");
    println!("cargo:rustc-link-lib=dylib=shoopdaloop_backend");

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
