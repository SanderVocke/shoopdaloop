pub mod logging;
pub mod qobj;
pub mod types;

pub mod shoop_rs_macros_tests;

fn register_qml_types_and_singletons() {
    use qobj::*;

    let mdl = String::from("ShoopDaLoop.Rust");
    qobj_file_io::register_qml_singleton (&mdl, "ShoopFileIO");
    qobj_os_utils::register_qml_singleton (&mdl, "ShoopOSUtils");
    qobj_release_focus_notifier::register_qml_singleton (&mdl, "ShoopReleaseFocusNotifier");
}

fn register_metatypes() {
    use types::*;
    let shoop_rust_callable_name = String::from("ShoopRustCallable");
    shoop_rust_callable::register_metatype(&shoop_rust_callable_name);
}

#[no_mangle]
pub extern "C" fn shoop_rust_init() {
    logging::init_logging().expect("Unable to initialize shoop_rs logging");

    register_metatypes();
    register_qml_types_and_singletons();
}