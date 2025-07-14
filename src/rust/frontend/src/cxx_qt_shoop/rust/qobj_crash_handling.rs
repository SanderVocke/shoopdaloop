use crate::cxx_qt_shoop::qobj_crash_handling_bridge::ffi::*;

pub fn register_qml_singleton(module_name: &str, type_name: &str) {
    let obj = make_unique_crash_handling();
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    register_qml_singleton_crash_handling(obj.as_ref().unwrap(), &mut mdl, 1, 0, &mut tp);
}

impl CrashHandling {
    pub fn set_json_toplevel_field(&self, key: QString, json: QString) {
        crashhandling::set_crash_json_toplevel_field(
            key.to_string().as_str(),
            serde_json::from_str(json.to_string().as_str()).unwrap(),
        );
    }

    pub fn set_json_tag(&self, tag: QString, value: QString) {
        crashhandling::set_crash_json_toplevel_field(
            tag.to_string().as_str(),
            serde_json::from_str(value.to_string().as_str()).unwrap(),
        );
    }
}
