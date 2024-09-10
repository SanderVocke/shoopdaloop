use std::path::PathBuf;
use std::env;

// For now, Rust "back-end" is just a set of C bindings to the
// C++ back-end.
fn main() {
    let header_path = "../../backend/libshoopdaloop.h";
    let lib_path =
        PathBuf::from(env::var("LIBSHOOPDALOOP_DIR").unwrap_or(String::from("../../libshoopdaloop")));
    let gen_lib_path = "src/codegen/libshoopdaloop.rs";

    // Generate Rust bindings
    let bindings = bindgen::Builder::default()
        .header(header_path)
        .generate()
        .expect("Unable to generate Rust bindings for libshoopdaloop.");

    bindings
        .write_to_file(gen_lib_path)
        .expect("Couldn't write Rust libshoopdaloop bindings.");

    println!("cargo:rerun-if-changed={}", header_path);

    println!("cargo:rustc-link-search=native={}", lib_path.display());
    // println!("cargo:rustc-link-lib=dylib=libshoopdaloop");
}
