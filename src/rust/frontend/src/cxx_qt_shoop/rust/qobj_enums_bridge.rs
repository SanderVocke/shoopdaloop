#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qmap.h");
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(
            QMap_QString_QVariant,
            loop_modes,
            cxx_name = "LoopMode",
            CONSTANT,
            READ
        )]
        #[qproperty(
            QMap_QString_QVariant,
            channel_modes,
            cxx_name = "ChannelMode",
            CONSTANT,
            READ
        )]
        #[qproperty(
            QMap_QString_QVariant,
            audio_driver_types,
            cxx_name = "AudioDriverType",
            CONSTANT,
            READ
        )]
        #[qproperty(
            QMap_QString_QVariant,
            port_directions,
            cxx_name = "PortDirection",
            CONSTANT,
            READ
        )]
        #[qproperty(
            QMap_QString_QVariant,
            fx_chain_types,
            cxx_name = "FxChainType",
            CONSTANT,
            READ
        )]
        #[qproperty(
            QMap_QString_QVariant,
            port_data_types,
            cxx_name = "PortDataType",
            CONSTANT,
            READ
        )]
        #[qproperty(
            QMap_QString_QVariant,
            port_connectabilities,
            cxx_name = "PortConnectability",
            CONSTANT,
            READ
        )]
        #[qproperty(
            QMap_QString_QVariant,
            key_event_types,
            cxx_name = "KeyEventType",
            CONSTANT,
            READ
        )]
        #[qproperty(
            QMap_QString_QVariant,
            loop_event_types,
            cxx_name = "LoopEventType",
            CONSTANT,
            READ
        )]
        #[qproperty(
            QMap_QString_QVariant,
            global_event_types,
            cxx_name = "GlobalEventType",
            CONSTANT,
            READ
        )]
        #[qproperty(QMap_QString_QVariant, keys, cxx_name = "Key", CONSTANT, READ)]
        #[qproperty(
            QMap_QString_QVariant,
            key_modifiers,
            cxx_name = "KeyModifier",
            CONSTANT,
            READ
        )]
        #[qproperty(i32, dont_wait_for_sync, cxx_name = "DontWaitForSync", CONSTANT, READ)]
        #[qproperty(
            i32,
            dont_align_to_sync_immediately,
            cxx_name = "DontAlignToSyncImmediately",
            CONSTANT,
            READ
        )]
        type Enums = super::EnumsRust;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/register_qml_type.h");

        #[rust_name = "register_qml_singleton_enums"]
        unsafe fn register_qml_singleton(
            inference_example: *mut Enums,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );
    }
}

use ffi::*;

pub struct EnumsRust {
    pub loop_modes: QMap_QString_QVariant,
    pub channel_modes: QMap_QString_QVariant,
    pub audio_driver_types: QMap_QString_QVariant,
    pub port_directions: QMap_QString_QVariant,
    pub fx_chain_types: QMap_QString_QVariant,
    pub port_data_types: QMap_QString_QVariant,
    pub port_connectabilities: QMap_QString_QVariant,
    pub key_event_types: QMap_QString_QVariant,
    pub loop_event_types: QMap_QString_QVariant,
    pub global_event_types: QMap_QString_QVariant,
    pub keys: QMap_QString_QVariant,
    pub key_modifiers: QMap_QString_QVariant,
    pub dont_wait_for_sync: i32,
    pub dont_align_to_sync_immediately: i32,
}
