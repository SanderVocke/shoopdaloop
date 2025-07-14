use crate::cxx_qt_shoop::qobj_crash_handling_bridge::ffi::*;

pub fn register_qml_singleton(module_name: &str, type_name: &str) {
    let obj = make_unique_crash_handling();
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    register_qml_singleton_crash_handling(obj.as_ref().unwrap(), &mut mdl, 1, 0, &mut tp);
}

impl CrashHandling {
    pub fn set_json_toplevel_field(&self, key: QString, json: QString) {
        let json_str = json.to_string();
        let maybe_structured = serde_json::from_str(&json_str);
        let value = if maybe_structured.is_ok() {
            maybe_structured.unwrap()
        } else {
            serde_json::json!(json_str)
        };

        crashhandling::set_crash_json_toplevel_field(key.to_string().as_str(), value);
    }

    pub fn set_json_tag(&self, tag: QString, json: QString) {
        let json_str = json.to_string();
        let maybe_structured = serde_json::from_str(&json_str);
        let value = if maybe_structured.is_ok() {
            maybe_structured.unwrap()
        } else {
            serde_json::json!(json_str)
        };

        crashhandling::set_crash_json_toplevel_field(tag.to_string().as_str(), value);
    }
}
