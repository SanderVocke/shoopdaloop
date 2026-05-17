fn main() {
    if cfg!(feature = "prebuild") {
        return;
    }

    cxx_build::bridges([
        "src/midi_state_tracker_cxx.rs",
        "src/midi_state_diff_tracker_cxx.rs",
        "src/midi_storage_cxx.rs",
        "src/midi_sorting_buffer_cxx.rs",
        "src/midi_port_cxx.rs",
        "src/midi_port_base_cxx.rs",
    ])
    .std("c++20")
    .compile("backend_rust_cxx");

    let out_dir = std::env::var_os("OUT_DIR").unwrap();
    let out_dir = std::path::Path::new(&out_dir);

    println!(
        "cargo:include={}",
        out_dir.join("cxxbridge/include").display()
    );
    println!("cargo:cxx_bridge_libdir={}", out_dir.display());

    println!("cargo:rerun-if-changed=src/midi_state_tracker_cxx.rs");
    println!("cargo:rerun-if-changed=src/midi_state_diff_tracker_cxx.rs");
    println!("cargo:rerun-if-changed=src/midi_storage_cxx.rs");
    println!("cargo:rerun-if-changed=src/midi_sorting_buffer_cxx.rs");
    println!("cargo:rerun-if-changed=src/midi_port_cxx.rs");
    println!("cargo:rerun-if-changed=src/midi_port_base_cxx.rs");
}
