fn main() {
    if cfg!(feature = "prebuild") {
        return;
    }

    cxx_build::bridge("src/cxx.rs")
        .std("c++20")
        .compile("refilling_pool");

    // Export the include path for downstream crates.
    // Cargo will turn this into `DEP_REFILLING_POOL_INCLUDE`.
    println!(
        "cargo:include={}",
        std::path::Path::new(&std::env::var_os("OUT_DIR").unwrap())
            .join("cxxbridge/include")
            .display()
    );

    println!("cargo:rerun-if-changed=src/cxx.rs");
}
