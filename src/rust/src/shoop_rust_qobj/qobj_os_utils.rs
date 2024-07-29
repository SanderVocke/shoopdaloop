#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    unsafe extern "RustQt" {
        #[qobject]
        // #[qml_element]
        type OSUtils = super::OSUtilsRust;
    }

    unsafe extern "RustQt" {
        #[qinvokable]
        fn cause_abort(self: &OSUtils);

        #[qinvokable]
        fn cause_panic(self: &OSUtils);

        #[qinvokable]
        fn cause_segfault(self: &OSUtils);

        #[qinvokable]
        fn get_env_var(self: &OSUtils, var: &QString) -> QString;
    }
}

use std::env;
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct OSUtilsRust {}

impl qobject::OSUtils {
    pub fn cause_abort(self: &qobject::OSUtils) {
        println!("OSUtils: triggering abort.");
        std::process::abort();
    }

    pub fn cause_segfault(self: &qobject::OSUtils) {
        println!("OSUtils: triggering segfault.");
        unsafe {
            let ptr: *const i32 = std::ptr::null();
            println!("{}", *ptr);
        }
    }

    pub fn cause_panic(self: &qobject::OSUtils) {
        println!("OSUtils: triggering panic.");
        panic!("");
    }

    pub fn get_env_var(self: &qobject::OSUtils, var: &QString) -> QString {
        match env::var(var.to_string()) {
            Ok (value) => return QString::from(&value),
            Err(_value) => return QString::from(""),
        }
    }
}