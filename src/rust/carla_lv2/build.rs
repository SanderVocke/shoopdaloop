fn main() {
    if cfg!(feature = "prebuild") {
        return;
    }

    // TODO: When the bridge files exist, call:
    // cxx_build::bridges(&["src/inner_bridge.rs", "src/outer_bridge.rs"])
    //     .std("c++20")
    //     .include("../../backend/internal/lv2")
    //     .include("../../backend/internal")
    //     .include("../../backend")
    //     .compile("carla_lv2_cxx");

    let out_dir = std::path::PathBuf::from(
        std::env::var("OUT_DIR").unwrap());

    // Export include path for CMake (dummy for now, real after bridges exist).
    println!(
        "cargo:include={}",
        out_dir.join("cxxbridge/include").display()
    );

    // Export library search path for CMake (dummy for now).
    println!(
        "cargo:cxx_bridge_libdir={}",
        out_dir.display()
    );

    // Export the path to the staticlib for CMake.
    println!(
        "cargo:rust_libdir={}",
        out_dir.join("../../../").display()
    );

    // Rebuild tracking (placeholder files — update when bridge modules exist).
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=build.rs");
}
