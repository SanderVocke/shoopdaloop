pub mod shoop_rs_qobj;
pub mod shoop_rs_macros_tests;

fn register_qml_types_and_singletons() {
    use shoop_rs_qobj::*;

    let mdl = String::from("ShoopDaLoop.Rust");
    qobj_file_io::register_qml_singleton (&mdl, "ShoopFileIO");
    qobj_os_utils::register_qml_singleton (&mdl, "ShoopOSUtils");
}

#[no_mangle]
pub extern "C" fn shoop_rust_init() {
    register_qml_types_and_singletons();
}