use crate::logging::macros::*;
const SHOOP_LOG_UNIT : &str = "Frontend.OSUtils";

#[cxx_qt::bridge]
pub mod qobj_os_utils {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    unsafe extern "RustQt" {
        #[qobject]
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

    unsafe extern "C++" {
        include!("cxx-shoop/make_unique.h");

        #[rust_name = "make_unique_osutils"]
        fn make_unique() -> UniquePtr<OSUtils>;
    }

    unsafe extern "C++" {
        include!("cxx-shoop/register_qml_type.h");

        #[rust_name = "register_qml_singleton_osutils"]
        fn register_qml_singleton(inference_example: &OSUtils,
                                  module_name : &mut String,
                                  version_major : i64, version_minor : i64,
                                  type_name : &mut String);
    }
}

use std::env;
use cxx_qt_lib::QString;
use qobj_os_utils::*;

pub fn register_qml_singleton(module_name : &str, type_name : &str) {
    let obj = make_unique_osutils();
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    register_qml_singleton_osutils(obj.as_ref().unwrap(), &mut mdl, 1, 0, &mut tp);
}

#[derive(Default)]
pub struct OSUtilsRust {}

#[allow(unreachable_code)]
impl OSUtils {
    pub fn cause_abort(self: &OSUtils) {
        warn!("Triggering abort.");
        std::process::abort();
    }

    pub fn cause_segfault(self: &OSUtils) {
        warn!("Triggering segfault.");
        unsafe {
            let ptr: *const i32 = std::ptr::null();
            println!("{}", *ptr);
        }
    }

    pub fn cause_panic(self: &OSUtils) {
        warn!("Triggering panic.");
        panic!("Manually triggered panic");
    }

    pub fn get_env_var(self: &OSUtils, var: &QString) -> QString {
        match env::var(var.to_string()) {
            Ok (value) => return QString::from(&value),
            Err(_value) => return QString::default(),
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_invalid_env_var() {
        let os_utils = make_unique_osutils();
        assert!(os_utils.get_env_var(&QString::from("DEADBEEF")).is_null());
    }

    #[test]
    fn test_valid_env_var() {
        let os_utils = make_unique_osutils();
        let home = env::var("HOME").unwrap();
        assert_eq!(
            os_utils.get_env_var(&QString::from("HOME")).to_string(),
            home
        );
    }
}