use shoop_rs_macros::*;

#[cxx_qt::bridge]
pub mod qobject {
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
}

use std::env;
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct OSUtilsRust {}

#[allow(unreachable_code)]
impl qobject::OSUtils {
    #[log_entry_exit]
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

    #[log_entry_exit]
    pub fn get_env_var(self: &qobject::OSUtils, var: &QString) -> QString {
        match env::var(var.to_string()) {
            Ok (value) => return QString::from(&value),
            Err(_value) => return QString::default(),
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use super::qobject::make_unique_osutils;
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