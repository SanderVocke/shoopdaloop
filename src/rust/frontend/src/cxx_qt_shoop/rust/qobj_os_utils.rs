use common::logging::macros::*;
shoop_log_unit!("Frontend.OSUtils");

pub use crate::cxx_qt_shoop::qobj_os_utils_bridge::ffi::OSUtils;
use crate::cxx_qt_shoop::qobj_os_utils_bridge::ffi::*;
use cxx_qt_lib::QString;
use std::env;

pub fn register_qml_singleton(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe { register_qml_singleton_osutils(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp); }
}

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
            Ok(value) => {
                debug!("Read env var {}: {}", var.to_string(), value);
                return QString::from(&value);
            }
            Err(_value) => {
                debug!("Failed to read env var {}", var.to_string());
                return QString::default();
            }
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
        let var: &str = if cfg!(windows) { "UserProfile" } else { "HOME" };
        let home = env::var(var).unwrap();
        assert_eq!(os_utils.get_env_var(&QString::from(var)).to_string(), home);
    }
}
