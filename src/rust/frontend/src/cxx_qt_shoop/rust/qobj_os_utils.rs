use common::logging::macros::*;
shoop_log_unit!("Frontend.OSUtils");

pub use crate::cxx_qt_shoop::qobj_os_utils_bridge::ffi::OSUtils;
use crate::cxx_qt_shoop::qobj_os_utils_bridge::ffi::*;
use cxx_qt_lib::{QString, QVariant};
use std::env;

pub fn register_qml_singleton(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe {
        register_qml_singleton_osutils(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp);
    }
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

    pub fn get_env_var_or_default(var: &str) -> QString {
        let var_string = var.to_string();
        let value = env::var(&var_string).unwrap_or_else(|_| {
            debug!("Failed to read env var {}", var_string);
            "".to_string()
        });
        debug!("Read env var {}: {}", var_string, value);
        QString::from(&value)
    }

    pub fn get_env_var(self: &OSUtils, var: &QString) -> QVariant {
        let var_string = var.to_string();
        match env::var(&var_string) {
            Ok(value) => {
                debug!("Read env var {}: {}", var_string, value);
                QVariant::from(&QString::from(&value))
            }
            Err(_value) => {
                debug!("Failed to read env var {}", var_string);
                QVariant::default()
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
        let home = env::var(var).unwrap_or("DEADBEEF");
        assert_eq!(
            os_utils
                .get_env_var(&QString::from(var))
                .value::<QString>()
                .unwrap_or(QString::default())
                .to_string(),
            home
        );
    }
}
