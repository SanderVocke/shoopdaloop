pub mod shoop_rs_qobj;
pub mod shoop_rs_macros_tests;

pub extern "C" fn register_qml_types_and_singletons() {
    use shoop_rs_qobj::*;

    let mdl = String::from("ShoopDaLoop.Rust");
    let vmaj = 1;
    let vmin = 0;
    qobj_file_io::register_qml_singleton_file_io (&mdl, vmaj, vmin, "ShoopFileIO");
    // qobj_os_utils::register_qml_singleton_os_utils (&mdl, vmaj, vmin, "ShoopOSUtils");
}