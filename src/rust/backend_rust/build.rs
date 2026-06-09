use std::fs;
use std::path::Path;

fn print_cxxbridge_sources_status(out_dir: &Path) {
    let cxx_src_dir = out_dir.join("cxxbridge/sources/backend_rust/src");
    let tracked = cxx_src_dir.join("midi_state_diff_tracker_cxx.rs.cc");
    println!(
        "cargo:warning=cxxbridge source dir exists: {} ({})",
        cxx_src_dir.display(),
        cxx_src_dir.exists()
    );
    println!(
        "cargo:warning=tracked cxxbridge source exists: {} ({})",
        tracked.display(),
        tracked.exists()
    );
    if cxx_src_dir.exists() {
        match fs::read_dir(&cxx_src_dir) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    println!(
                        "cargo:warning=cxxbridge source file: {}",
                        entry.path().display()
                    );
                }
            }
            Err(e) => {
                println!(
                    "cargo:warning=failed to list cxxbridge source dir {}: {}",
                    cxx_src_dir.display(),
                    e
                );
            }
        }
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) {
    if let Err(e) = fs::create_dir_all(dst) {
        panic!("Failed to create dir {}: {}", dst.display(), e);
    }
    for entry in
        fs::read_dir(src).unwrap_or_else(|e| panic!("Failed to read dir {}: {}", src.display(), e))
    {
        let entry = entry
            .unwrap_or_else(|e| panic!("Failed to read dir entry in {}: {}", src.display(), e));
        let ty = entry.file_type().unwrap_or_else(|e| {
            panic!(
                "Failed to get file type for {}: {}",
                entry.path().display(),
                e
            )
        });
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &dst_path);
        } else if ty.is_file() {
            fs::copy(entry.path(), &dst_path).unwrap_or_else(|e| {
                panic!(
                    "Failed to copy {} -> {}: {}",
                    entry.path().display(),
                    dst_path.display(),
                    e
                )
            });
        }
    }
}

fn main() {
    if cfg!(feature = "prebuild") {
        return;
    }

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let backend_include = format!("{}/../../backend", manifest_dir);

    let out_dir = std::env::var_os("OUT_DIR").unwrap();
    let out_dir = std::path::Path::new(&out_dir);

    let mut build = cxx_build::bridges([
        "src/i_processor_cxx.rs",
        "src/processor_cxx.rs",
        "src/cpp_midi_port_cxx.rs",
        "src/audio_midi_driver_bridge_cxx.rs",
        "src/audio_midi_driver_cxx.rs",
        "src/backend_api_cxx.rs",
        "src/command_queue_cxx.rs",
        "src/midi_state_diff_tracker_cxx.rs",
        "src/midi_storage_cxx.rs",
        "src/midi_sorting_buffer_cxx.rs",
        "src/midi_port_cxx.rs",
        "src/midi_port_base_cxx.rs",
        "src/dummy_midi_port_cxx.rs",
        "src/decoupled_midi_port_cxx.rs",
        "src/dummy_external_connections_cxx.rs",
        "src/port_core_cxx.rs",
        "src/dummy_audio_port_cxx.rs",
        "src/internal_audio_port_cxx.rs",
        "src/internal_midi_port_cxx.rs",
        "src/midi_buffering_input_port_cxx.rs",
        "src/audio_port_cxx.rs",
        "src/midi_state_tracker_cxx.rs",
        "src/refilling_pool_cxx.rs",
        "src/dummy_audio_midi_driver_cxx.rs",
        "src/rust_bridge_object_test_cxx.rs",
    ]);

    build
        .file(format!("{}/internal/CommandToken.cpp", backend_include))
        .file("cxx/bridge_object_fallback.cpp")
        .include(&backend_include);

    println!("cargo:rerun-if-env-changed=SHOOP_BACKEND_RUST_CXX_INCLUDE_DIRS");
    if let Ok(extra_includes) = std::env::var("SHOOP_BACKEND_RUST_CXX_INCLUDE_DIRS") {
        for include in extra_includes.split('|').filter(|s| !s.is_empty()) {
            println!(
                "cargo:warning=Adding C++ include dir from CMake: {}",
                include
            );
            build.include(include);
        }
    }

    build.std("c++20").compile("backend_rust_cxx");

    print_cxxbridge_sources_status(out_dir);

    println!(
        "cargo:include={}",
        out_dir.join("cxxbridge/include").display()
    );
    println!("cargo:cxx_bridge_libdir={}", out_dir.display());

    if let Ok(corrosion_build_dir) = std::env::var("CORROSION_BUILD_DIR") {
        let corrosion_dir = Path::new(&corrosion_build_dir);
        let src_include = out_dir.join("cxxbridge/include");
        let dst_include = corrosion_dir.join("backend_rust_cxx_include");
        if src_include.exists() {
            copy_dir_recursive(&src_include, &dst_include);
            println!(
                "cargo:warning=Copied cxxbridge include dir to {}",
                dst_include.display()
            );
        }

        let src_cxx_lib = out_dir.join("libbackend_rust_cxx.a");
        let dst_cxx_lib = corrosion_dir.join("libbackend_rust_cxx.a");
        if src_cxx_lib.exists() {
            fs::copy(&src_cxx_lib, &dst_cxx_lib).unwrap_or_else(|e| {
                panic!(
                    "Failed to copy {} -> {}: {}",
                    src_cxx_lib.display(),
                    dst_cxx_lib.display(),
                    e
                )
            });
            println!(
                "cargo:warning=Copied cxx bridge lib to {}",
                dst_cxx_lib.display()
            );
        }
    }

    println!("cargo:rerun-if-changed=src/i_processor_cxx.rs");
    println!("cargo:rerun-if-changed=src/processor_cxx.rs");
    println!("cargo:rerun-if-changed=src/cpp_midi_port_cxx.rs");
    println!("cargo:rerun-if-changed=src/audio_midi_driver_bridge_cxx.rs");
    println!("cargo:rerun-if-changed=src/audio_midi_driver_cxx.rs");
    println!("cargo:rerun-if-changed=src/backend_api_cxx.rs");

    println!("cargo:rerun-if-changed=src/command_queue_cxx.rs");
    println!(
        "cargo:rerun-if-changed={}/internal/MidiPortCxxBridge.h",
        backend_include
    );
    println!(
        "cargo:rerun-if-changed={}/internal/CommandToken.cpp",
        backend_include
    );
    println!(
        "cargo:rerun-if-changed={}/internal/CommandToken.h",
        backend_include
    );
    println!("cargo:rerun-if-changed=src/midi_state_diff_tracker_cxx.rs");
    println!("cargo:rerun-if-changed=src/midi_storage_cxx.rs");
    println!("cargo:rerun-if-changed=src/midi_sorting_buffer_cxx.rs");
    println!("cargo:rerun-if-changed=src/midi_port_cxx.rs");
    println!("cargo:rerun-if-changed=src/midi_port_base_cxx.rs");
    println!("cargo:rerun-if-changed=src/dummy_midi_port_cxx.rs");
    println!("cargo:rerun-if-changed=src/decoupled_midi_port_cxx.rs");
    println!("cargo:rerun-if-changed=src/internal_audio_port_cxx.rs");
    println!("cargo:rerun-if-changed=src/internal_midi_port_cxx.rs");
    println!("cargo:rerun-if-changed=src/midi_buffering_input_port_cxx.rs");
    println!("cargo:rerun-if-changed=src/audio_port_cxx.rs");
    println!("cargo:rerun-if-changed=src/refilling_pool_cxx.rs");
    println!("cargo:rerun-if-changed=src/dummy_audio_midi_driver_cxx.rs");
    println!("cargo:rerun-if-changed=src/rust_bridge_object.rs");
    println!("cargo:rerun-if-changed=src/rust_bridge_object_test_cxx.rs");
}
