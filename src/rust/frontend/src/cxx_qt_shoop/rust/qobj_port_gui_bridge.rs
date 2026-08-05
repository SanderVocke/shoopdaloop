use std::collections::HashSet;

use crate::any_backend_port::{AnyBackendPort, AnyBackendPortState};
use common::logging::macros::*;
use cxx_qt_lib_shoop::qpointer::QPointerQObject;
use shoop_engine::{PortConnectability, PortDataType};

shoop_log_unit!("Frontend.Port");

#[cxx_qt::bridge]
pub mod ffi {
    extern "C++" {
        #[doc(hidden)]
        #[namespace = ""]
        type QObject = cxx_qt::QObject;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");
        type QQuickItem = cxx_qt_lib_shoop::qquickitem::QQuickItem;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qlist.h");
        type QList_QVariant = cxx_qt_lib::QList<cxx_qt_lib::QVariant>;
        type QList_f32 = cxx_qt_lib::QList<f32>;
        type QList_QString = cxx_qt_lib::QList<cxx_qt_lib::QString>;

        include!("cxx-qt-lib/qmap.h");
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QQuickItem]
        // Engine -> frontend properties
        #[qproperty(bool, initialized, READ=get_initialized, NOTIFY=initialized_changed)]
        #[qproperty(QString, name, READ=get_name, NOTIFY=name_changed)]
        #[qproperty(bool, muted, READ=get_muted, NOTIFY=muted_changed)]
        #[qproperty(bool, passthrough_muted, READ=get_passthrough_muted, NOTIFY=passthrough_muted_changed)]
        #[qproperty(f32, audio_input_peak, READ=get_audio_input_peak, NOTIFY=audio_input_peak_changed)]
        #[qproperty(f32, audio_output_peak, READ=get_audio_output_peak, NOTIFY=audio_output_peak_changed)]
        #[qproperty(i32, midi_n_input_events, READ=get_midi_n_input_events, NOTIFY=midi_n_input_events_changed)]
        #[qproperty(i32, midi_n_output_events, READ=get_midi_n_output_events, NOTIFY=midi_n_output_events_changed)]
        #[qproperty(i32, midi_n_input_notes_active, READ=get_midi_n_input_notes_active, NOTIFY=midi_n_input_notes_active_changed)]
        #[qproperty(i32, midi_n_output_notes_active, READ=get_midi_n_output_notes_active, NOTIFY=midi_n_output_notes_active_changed)]
        #[qproperty(i32, n_ringbuffer_samples, READ=get_n_ringbuffer_samples, NOTIFY=n_ringbuffer_samples_changed)]
        #[qproperty(f32, audio_gain, READ=get_audio_gain, NOTIFY=audio_gain_changed)]
        // Frontend -> Backend properties
        #[qproperty(bool, is_midi, READ=get_is_midi, WRITE=set_is_midi, NOTIFY=is_midi_changed)]
        #[qproperty(*mut QObject, backend, READ, WRITE=set_backend, NOTIFY=backend_changed)]
        #[qproperty(*mut QObject, maybe_fx_chain, READ=get_maybe_fx_chain, WRITE=set_fx_chain, NOTIFY=fx_chain_changed)]
        #[qproperty(i32, fx_chain_port_idx, READ=get_fx_chain_port_idx, WRITE=set_fx_chain_port_idx, NOTIFY=fx_chain_port_idx_changed)]
        #[qproperty(QString, name_hint, READ=get_name_hint, WRITE=set_name_hint, NOTIFY=name_hint_changed)]
        #[qproperty(i32, input_connectability, READ=get_input_connectability, WRITE=set_input_connectability, NOTIFY=input_connectability_changed)]
        #[qproperty(i32, output_connectability, READ=get_output_connectability, WRITE=set_output_connectability, NOTIFY=output_connectability_changed)]
        #[qproperty(bool, is_internal, READ=get_is_internal, WRITE=set_is_internal, NOTIFY=is_internal_changed)]
        #[qproperty(QList_QVariant, internal_port_connections, READ, WRITE=set_internal_port_connections, NOTIFY=internal_port_connections_changed)]
        #[qproperty(i32, min_n_ringbuffer_samples, READ=get_min_n_ringbuffer_samples, WRITE=set_min_n_ringbuffer_samples, NOTIFY=min_n_ringbuffer_samples_changed)]
        type PortGui = super::PortGuiRust;

        pub fn initialize_impl(self: Pin<&mut PortGui>);

        #[qinvokable]
        pub fn get_initialized(self: &PortGui) -> bool;
        #[qinvokable]
        pub fn get_name(self: &PortGui) -> QString;
        #[qinvokable]
        pub fn get_muted(self: &PortGui) -> bool;
        #[qinvokable]
        pub fn get_passthrough_muted(self: &PortGui) -> bool;
        #[qinvokable]
        pub fn get_audio_input_peak(self: &PortGui) -> f32;
        #[qinvokable]
        pub fn get_audio_output_peak(self: &PortGui) -> f32;
        #[qinvokable]
        pub fn get_midi_n_input_events(self: &PortGui) -> i32;
        #[qinvokable]
        pub fn get_midi_n_output_events(self: &PortGui) -> i32;
        #[qinvokable]
        pub fn get_midi_n_input_notes_active(self: &PortGui) -> i32;
        #[qinvokable]
        pub fn get_midi_n_output_notes_active(self: &PortGui) -> i32;

        #[qinvokable]
        pub fn connect_external_port(self: Pin<&mut PortGui>, name: QString);

        #[qinvokable]
        pub fn disconnect_external_port(self: Pin<&mut PortGui>, name: QString);

        #[qinvokable]
        pub fn get_connections_state(self: Pin<&mut PortGui>) -> QMap_QString_QVariant;

        #[qinvokable]
        pub fn get_connected_external_ports(self: Pin<&mut PortGui>) -> QList_QString;

        #[qinvokable]
        pub fn try_make_connections(self: Pin<&mut PortGui>, connections: QList_QString);

        #[qinvokable]
        pub unsafe fn set_backend(self: Pin<&mut PortGui>, backend: *mut QObject);

        #[qinvokable]
        pub fn set_name_hint(self: Pin<&mut PortGui>, name_hint: QString);

        #[qinvokable]
        pub fn set_input_connectability(self: Pin<&mut PortGui>, input_connectability: i32);

        #[qinvokable]
        pub fn set_output_connectability(self: Pin<&mut PortGui>, output_connectability: i32);

        #[qinvokable]
        pub fn set_is_internal(self: Pin<&mut PortGui>, is_internal: bool);

        #[qinvokable]
        pub unsafe fn set_fx_chain(self: Pin<&mut PortGui>, fx_chain: *mut QObject);

        #[qinvokable]
        pub fn set_fx_chain_port_idx(self: Pin<&mut PortGui>, fx_chain_port_idx: i32);

        #[qinvokable]
        pub fn set_internal_port_connections(
            self: Pin<&mut PortGui>,
            internal_port_connections: QList_QVariant,
        );

        #[qinvokable]
        pub fn update_internal_port_connections(self: Pin<&mut PortGui>);

        #[qinvokable]
        pub fn set_min_n_ringbuffer_samples(self: Pin<&mut PortGui>, n_ringbuffer_samples: i32);

        #[qinvokable]
        pub fn push_audio_gain(self: Pin<&mut PortGui>, audio_gain: f32);

        #[qinvokable]
        pub fn push_muted(self: Pin<&mut PortGui>, muted: bool);

        #[qinvokable]
        pub fn push_passthrough_muted(self: Pin<&mut PortGui>, passthrough_muted: bool);

        #[qinvokable]
        pub fn set_is_midi(self: Pin<&mut PortGui>, is_midi: bool);

        #[qinvokable]
        pub fn dummy_queue_audio_data(self: Pin<&mut PortGui>, audio_data: QList_f32);

        #[qinvokable]
        pub fn dummy_dequeue_audio_data(self: Pin<&mut PortGui>, n: i32) -> QList_f32;

        #[qinvokable]
        pub fn dummy_request_data(self: Pin<&mut PortGui>, n: i32);

        #[qinvokable]
        pub fn dummy_queue_midi_msgs(self: Pin<&mut PortGui>, midi_msgs: QList_QVariant);

        #[qinvokable]
        pub fn dummy_dequeue_midi_msgs(self: Pin<&mut PortGui>) -> QList_QVariant;

        #[qinvokable]
        pub fn dummy_clear_queues(self: Pin<&mut PortGui>);

        #[qinvokable]
        pub fn get_n_ringbuffer_samples(self: Pin<&mut PortGui>) -> i32;

        #[qinvokable]
        pub fn get_is_midi(self: Pin<&mut PortGui>) -> bool;

        #[qinvokable]
        pub fn get_maybe_fx_chain(self: Pin<&mut PortGui>) -> *mut QObject;

        #[qinvokable]
        pub fn get_fx_chain_port_idx(self: Pin<&mut PortGui>) -> i32;

        #[qinvokable]
        pub fn get_name_hint(self: Pin<&mut PortGui>) -> QString;

        #[qinvokable]
        pub fn get_input_connectability(self: Pin<&mut PortGui>) -> i32;

        #[qinvokable]
        pub fn get_output_connectability(self: Pin<&mut PortGui>) -> i32;

        #[qinvokable]
        pub fn get_is_internal(self: Pin<&mut PortGui>) -> bool;

        #[qinvokable]
        pub fn get_audio_gain(self: Pin<&mut PortGui>) -> f32;

        #[qinvokable]
        pub fn get_min_n_ringbuffer_samples(self: Pin<&mut PortGui>) -> i32;

        #[qinvokable]
        pub fn update(self: Pin<&mut PortGui>);

        #[qinvokable]
        pub fn maybe_initialize_backend(self: Pin<&mut PortGui>) -> bool;

        #[qinvokable]
        pub fn deinit(self: Pin<&mut PortGui>);

        #[qsignal]
        pub unsafe fn state_changed(
            self: Pin<&mut PortGui>,
            initialized: bool,
            name: QString,
            muted: bool,
            passthrough_muted: bool,
            audio_gain: f32,
            audio_input_peak: f32,
            audio_output_peak: f32,
            midi_n_input_events: i32,
            midi_n_output_events: i32,
            midi_n_input_notes_active: i32,
            midi_n_output_notes_active: i32,
            n_ringbuffer_samples: i32,
        );

        #[qsignal]
        pub unsafe fn connections_state_changed(
            self: Pin<&mut PortGui>,
            state: QMap_QString_QVariant,
        );

        #[qsignal]
        pub unsafe fn backend_changed(self: Pin<&mut PortGui>, backend: *mut QObject);

        #[qsignal]
        pub unsafe fn name_hint_changed(self: Pin<&mut PortGui>, name_hint: QString);

        #[qsignal]
        pub unsafe fn input_connectability_changed(
            self: Pin<&mut PortGui>,
            input_connectability: i32,
        );

        #[qsignal]
        pub unsafe fn output_connectability_changed(
            self: Pin<&mut PortGui>,
            output_connectability: i32,
        );

        #[qsignal]
        pub unsafe fn is_internal_changed(self: Pin<&mut PortGui>, is_internal: bool);

        #[qsignal]
        pub unsafe fn internal_port_connections_changed(
            self: Pin<&mut PortGui>,
            internal_port_connections: QList_QVariant,
        );

        #[qsignal]
        pub unsafe fn n_ringbuffer_samples_changed(
            self: Pin<&mut PortGui>,
            n_ringbuffer_samples: i32,
        );

        #[qsignal]
        pub unsafe fn min_n_ringbuffer_samples_changed(
            self: Pin<&mut PortGui>,
            min_n_ringbuffer_samples: i32,
        );

        #[qsignal]
        pub unsafe fn audio_gain_changed(self: Pin<&mut PortGui>, audio_gain: f32);

        #[qsignal]
        pub unsafe fn initialized_changed(self: Pin<&mut PortGui>, initialized: bool);

        #[qsignal]
        pub unsafe fn name_changed(self: Pin<&mut PortGui>, name: QString);

        #[qsignal]
        pub unsafe fn muted_changed(self: Pin<&mut PortGui>, muted: bool);

        #[qsignal]
        pub unsafe fn passthrough_muted_changed(self: Pin<&mut PortGui>, passthrough_muted: bool);

        #[qsignal]
        pub unsafe fn audio_input_peak_changed(self: Pin<&mut PortGui>, audio_input_peak: f32);

        #[qsignal]
        pub unsafe fn audio_output_peak_changed(self: Pin<&mut PortGui>, audio_output_peak: f32);

        #[qsignal]
        pub unsafe fn midi_n_input_events_changed(
            self: Pin<&mut PortGui>,
            midi_n_input_events: i32,
        );

        #[qsignal]
        pub unsafe fn fx_chain_changed(self: Pin<&mut PortGui>, fx_chain: *mut QObject);

        #[qsignal]
        pub unsafe fn fx_chain_port_idx_changed(self: Pin<&mut PortGui>, fx_chain_port_idx: i32);

        #[qsignal]
        pub unsafe fn midi_n_output_events_changed(
            self: Pin<&mut PortGui>,
            midi_n_output_events: i32,
        );

        #[qsignal]
        pub unsafe fn midi_n_input_notes_active_changed(
            self: Pin<&mut PortGui>,
            midi_n_input_notes_active: i32,
        );

        #[qsignal]
        pub unsafe fn midi_n_output_notes_active_changed(
            self: Pin<&mut PortGui>,
            midi_n_output_notes_active: i32,
        );

        #[qsignal]
        pub unsafe fn is_midi_changed(self: Pin<&mut PortGui>, is_midi: bool);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qobject.h");
        include!("cxx-qt-lib-shoop/qquickitem.h");

        #[rust_name = "qquickitem_from_ref_port_gui"]
        unsafe fn qquickitemFromRef(obj: &PortGui) -> &QQuickItem;

        #[rust_name = "qquickitem_from_ptr_port_gui"]
        unsafe fn qquickitemFromPtr(obj: *mut PortGui) -> *mut QQuickItem;

        #[rust_name = "from_qobject_ref_port_gui"]
        unsafe fn fromQObjectRef(obj: &QObject, output: *mut *const PortGui);

        #[rust_name = "from_qobject_mut_port_gui"]
        unsafe fn fromQObjectMut(obj: Pin<&mut QObject>, output: *mut *mut PortGui);

        include!("cxx-qt-lib-shoop/register_qml_type.h");
        #[rust_name = "register_qml_type_port_gui"]
        unsafe fn register_qml_type(
            inference_example: *mut PortGui,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );

        #[rust_name = "port_gui_qobject_from_ptr"]
        unsafe fn qobjectFromPtr(obj: *mut PortGui) -> *mut QObject;

        #[rust_name = "port_gui_qobject_from_ref"]
        fn qobjectFromRef(obj: &PortGui) -> &QObject;
    }

    impl cxx_qt::Constructor<(*mut QQuickItem,), NewArguments = (*mut QQuickItem,)> for PortGui {}
    impl cxx_qt::Constructor<()> for PortGui {}
}

pub use ffi::PortGui;
use ffi::*;

pub struct PortGuiRust {
    // Properties
    pub initialized: bool,
    pub backend: *mut QObject,
    pub backend_guard: cxx::UniquePtr<QPointerQObject>,
    pub internal_port_connections: QList_QVariant,

    // Other fields
    pub maybe_backend_port: Option<AnyBackendPort>,
    pub prev_state: AnyBackendPortState,
    pub name_hint: Option<QString>,
    pub input_connectability: Option<PortConnectability>,
    pub output_connectability: Option<PortConnectability>,
    pub is_internal: Option<bool>,
    pub port_type: Option<PortDataType>,
    pub fx_chain: *mut QObject,
    pub fx_chain_guard: cxx::UniquePtr<QPointerQObject>,
    pub fx_chain_port_idx: Option<i32>,
    pub min_n_ringbuffer_samples: Option<i32>,
    pub internally_connected_ports: HashSet<*mut QObject>,
}

impl Default for PortGuiRust {
    fn default() -> PortGuiRust {
        PortGuiRust {
            initialized: false,
            backend: std::ptr::null_mut(),
            backend_guard: cxx::UniquePtr::null(),
            name_hint: None,
            input_connectability: None,
            output_connectability: None,
            is_internal: None,
            internal_port_connections: QList_QVariant::default(),
            port_type: None,
            fx_chain: std::ptr::null_mut(),
            fx_chain_guard: cxx::UniquePtr::null(),
            fx_chain_port_idx: None,
            maybe_backend_port: None,
            prev_state: AnyBackendPortState::default(),
            min_n_ringbuffer_samples: None,
            internally_connected_ports: HashSet::default(),
        }
    }
}

impl cxx_qt_lib_shoop::qobject::FromQObject for PortGui {
    unsafe fn ptr_from_qobject_ref(obj: &cxx_qt::QObject) -> *const Self {
        let mut output: *const Self = std::ptr::null();
        from_qobject_ref_port_gui(obj, &mut output as *mut *const Self);
        output
    }

    unsafe fn ptr_from_qobject_mut(obj: std::pin::Pin<&mut cxx_qt::QObject>) -> *mut Self {
        let mut output: *mut Self = std::ptr::null_mut();
        from_qobject_mut_port_gui(obj, &mut output as *mut *mut Self);
        output
    }
}

impl cxx_qt_lib_shoop::qquickitem::AsQQuickItem for PortGui {
    unsafe fn mut_qquickitem_ptr(&mut self) -> *mut QQuickItem {
        qquickitem_from_ptr_port_gui(self as *mut Self)
    }

    unsafe fn ref_qquickitem_ptr(&self) -> *const QQuickItem {
        qquickitem_from_ref_port_gui(self) as *const QQuickItem
    }
}

impl cxx_qt_lib_shoop::qquickitem::IsQQuickItem for PortGui {}

impl cxx_qt::Constructor<(*mut QQuickItem,)> for PortGui {
    type BaseArguments = (*mut QQuickItem,);
    type InitializeArguments = ();
    type NewArguments = (*mut QQuickItem,);

    fn route_arguments(
        args: (*mut QQuickItem,),
    ) -> (
        Self::NewArguments,
        Self::BaseArguments,
        Self::InitializeArguments,
    ) {
        (args, args, ())
    }

    fn new(_parent: (*mut QQuickItem,)) -> PortGuiRust {
        PortGuiRust::default()
    }

    fn initialize(self: std::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        PortGui::initialize_impl(self);
    }
}

impl cxx_qt::Constructor<()> for PortGui {
    type BaseArguments = ();
    type InitializeArguments = ();
    type NewArguments = ();

    fn route_arguments(
        args: (),
    ) -> (
        Self::NewArguments,
        Self::BaseArguments,
        Self::InitializeArguments,
    ) {
        (args, args, ())
    }

    fn new(_parent: ()) -> PortGuiRust {
        PortGuiRust::default()
    }

    fn initialize(self: std::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        PortGui::initialize_impl(self);
    }
}
