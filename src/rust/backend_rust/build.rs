fn main() {
    if cfg!(feature = "prebuild") {
        return;
    }

    cxx_build::bridge("src/cxx.rs")
        .std("c++20")
        .compile("backend_rust_cxx");

    let out_dir = std::env::var_os("OUT_DIR").unwrap();
    let out_dir = std::path::Path::new(&out_dir);

    println!(
        "cargo:include={}",
        out_dir.join("cxxbridge/include").display()
    );
    println!("cargo:cxx_bridge_libdir={}", out_dir.display());

    println!("cargo:rerun-if-changed=src/cxx.rs");
}
