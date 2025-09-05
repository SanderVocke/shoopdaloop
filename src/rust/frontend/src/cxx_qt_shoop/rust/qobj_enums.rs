pub use crate::cxx_qt_shoop::qobj_enums_bridge::ffi::Enums;
use crate::cxx_qt_shoop::qobj_enums_bridge::ffi::*;
use crate::cxx_qt_shoop::qobj_enums_bridge::EnumsRust;
use crate::cxx_qt_shoop::qobj_session_control_handler::{
    GlobalEventType, KeyEventType, LoopEventType,
};
use backend_bindings::AudioDriverType;
use backend_bindings::ChannelMode;
use backend_bindings::FXChainType;
use backend_bindings::LoopMode;
use backend_bindings::PortConnectabilityKind;
use backend_bindings::PortDataType;
use backend_bindings::PortDirection;
use cxx_qt_lib::{QMap, QMapPair_QString_QVariant, QString, QVariant};

macro_rules! enum_to_map {
    ($enum:ty) => {
        || -> QMap<QMapPair_QString_QVariant> {
            let mut map: QMap<QMapPair_QString_QVariant> = QMap::default();
            for v in enum_iterator::all::<$enum>() {
                let key = QString::from(format!("{v:?}"));
                let value: i32 = v as i32;
                map.insert(key, QVariant::from(&value));
            }
            map
        }()
    };
}

impl Default for EnumsRust {
    fn default() -> Self {
        Self {
            loop_modes: enum_to_map!(LoopMode),
            channel_modes: enum_to_map!(ChannelMode),
            audio_driver_types: enum_to_map!(AudioDriverType),
            port_directions: enum_to_map!(PortDirection),
            fx_chain_types: enum_to_map!(FXChainType),
            port_data_types: enum_to_map!(PortDataType),
            port_connectabilities: enum_to_map!(PortConnectabilityKind),
            key_event_types: enum_to_map!(KeyEventType),
            loop_event_types: enum_to_map!(LoopEventType),
            global_event_types: enum_to_map!(GlobalEventType),
            keys: enum_to_map!(qt_header_bindings::Qt_Key),
            key_modifiers: enum_to_map!(qt_header_bindings::Qt_KeyboardModifier),
            dont_wait_for_sync: -1,
            dont_align_to_sync_immediately: -1,
        }
    }
}

pub fn register_qml_singleton(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe {
        register_qml_singleton_enums(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp);
    }
}
