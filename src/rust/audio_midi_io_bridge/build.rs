fn main() {
    if cfg!(feature = "prebuild") {
        return;
    }

    cxx_build::bridge("src/interface/bridge.rs")
        .std("c++20")
        .compile("audio_midi_io_cxx");

    let out_dir = std::env::var_os("OUT_DIR").unwrap();
    let out_dir = std::path::Path::new(&out_dir);

    // Export the include path for downstream crates.
    // Cargo will turn this into `DEP_REFILLING_POOL_INCLUDE`.
    println!(
        "cargo:include={}",
        out_dir.join("cxxbridge/include").display()
    );

    // Export the library search path for downstream crates
    // Cargo will turn this into `DEP_REFILLING_POOL_LIBDIR`.
    println!("cargo:cxx_bridge_libdir={}", out_dir.display());
    let target_dir = out_dir.join("../../../");
    println!("cargo:rust_libdir={}", target_dir.display());

    println!("cargo:rerun-if-changed=src/interface/bridge.rs");
}
