use common::logging::macros::*;

shoop_log_unit!("Frontend.Port");

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");
        type QQuickItem = cxx_qt_lib_shoop::qquickitem::QQuickItem;
        type QObject = cxx_qt_lib_shoop::qobject::QObject;

        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qlist.h");
        type QList_QVariant = cxx_qt_lib::QList<cxx_qt_lib::QVariant>;
        type QList_f64 = cxx_qt_lib::QList<f64>;
        type QList_QString = cxx_qt_lib::QList<cxx_qt_lib::QString>;

        include!("cxx-qt-lib/qmap.h");
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QQuickItem]
        // Backend -> Frontend properties
        #[qproperty(bool, initialized, READ, NOTIFY=initialized_changed)]
        #[qproperty(QString, name, READ, NOTIFY=name_changed)]
        #[qproperty(bool, muted, READ, NOTIFY=muted_changed)]
        #[qproperty(bool, passthrough_muted, READ, NOTIFY=passthrough_muted_changed)]
        #[qproperty(f64, audio_input_peak, READ, NOTIFY=audio_input_peak_changed)]
        #[qproperty(f64, audio_output_peak, READ, NOTIFY=audio_output_peak_changed)]
        #[qproperty(i32, midi_n_input_events, READ, NOTIFY=midi_n_input_events_changed)]
        #[qproperty(i32, midi_n_output_events, READ, NOTIFY=midi_n_output_events_changed)]
        #[qproperty(i32, midi_n_input_notes_active, READ, NOTIFY=midi_n_input_notes_active_changed)]
        #[qproperty(i32, midi_n_output_notes_active, READ, NOTIFY=midi_n_output_notes_active_changed)]
        // Frontend -> Backend properties
        #[qproperty(*mut QObject, backend, READ, WRITE=set_backend, NOTIFY=backend_changed)]
        #[qproperty(QString, name_hint, READ, WRITE=set_name_hint, NOTIFY=name_hint_changed)]
        #[qproperty(i32, input_connectability, READ, WRITE=set_input_connectability, NOTIFY=input_connectability_changed)]
        #[qproperty(i32, output_connectability, READ, WRITE=set_output_connectability, NOTIFY=output_connectability_changed)]
        #[qproperty(bool, is_internal, READ, WRITE=set_is_internal, NOTIFY=is_internal_changed)]
        #[qproperty(QList_QVariant, internal_port_connections, READ, WRITE=set_internal_port_connections, NOTIFY=internal_port_connections_changed)]
        #[qproperty(i32, n_ringbuffer_samples, READ, WRITE=set_n_ringbuffer_samples, NOTIFY=n_ringbuffer_samples_changed)]
        #[qproperty(f64, audio_gain, READ, WRITE=set_audio_gain, NOTIFY=audio_gain_changed)]
        // Other properties
        //#[qproperty(*mut QObject, backend_port_wrapper, READ=get_backend_port_wrapper)]
        type PortGui = super::PortGuiRust;

        pub fn initialize_impl(self: Pin<&mut PortGui>);

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
        pub fn set_internal_port_connections(
            self: Pin<&mut PortGui>,
            internal_port_connections: QList_QVariant,
        );

        #[qinvokable]
        pub fn set_n_ringbuffer_samples(self: Pin<&mut PortGui>, n_ringbuffer_samples: i32);

        #[qinvokable]
        pub fn set_audio_gain(self: Pin<&mut PortGui>, audio_gain: f64);

        #[qinvokable]
        pub fn dummy_queue_audio_data(self: Pin<&mut PortGui>, audio_data: QList_f64);

        #[qinvokable]
        pub fn dummy_dequeue_audio_data(self: Pin<&mut PortGui>) -> QList_f64;

        #[qinvokable]
        pub fn dummy_request_data(self: Pin<&mut PortGui>, n: i32);

        #[qinvokable]
        pub fn dummy_queue_midi_msgs(self: Pin<&mut PortGui>, midi_msgs: QList_QVariant);

        #[qinvokable]
        pub fn dummy_dequeue_midi_msgs(self: Pin<&mut PortGui>) -> QList_QVariant;

        #[qinvokable]
        pub fn dummy_clear_queues(self: Pin<&mut PortGui>);

        #[qsignal]
        #[cxx_name = "backendChanged"]
        pub unsafe fn backend_changed(self: Pin<&mut PortGui>, backend: *mut QObject);

        #[qsignal]
        #[cxx_name = "nameHintChanged"]
        pub unsafe fn name_hint_changed(self: Pin<&mut PortGui>, name_hint: QString);

        #[qsignal]
        #[cxx_name = "inputConnectabilityChanged"]
        pub unsafe fn input_connectability_changed(
            self: Pin<&mut PortGui>,
            input_connectability: i32,
        );

        #[qsignal]
        #[cxx_name = "outputConnectabilityChanged"]
        pub unsafe fn output_connectability_changed(
            self: Pin<&mut PortGui>,
            output_connectability: i32,
        );

        #[qsignal]
        #[cxx_name = "isInternalChanged"]
        pub unsafe fn is_internal_changed(self: Pin<&mut PortGui>, is_internal: bool);

        #[qsignal]
        #[cxx_name = "internalPortConnectionsChanged"]
        pub unsafe fn internal_port_connections_changed(
            self: Pin<&mut PortGui>,
            internal_port_connections: QList_QVariant,
        );

        #[qsignal]
        #[cxx_name = "nRingbufferSamplesChanged"]
        pub unsafe fn n_ringbuffer_samples_changed(
            self: Pin<&mut PortGui>,
            n_ringbuffer_samples: i32,
        );

        #[qsignal]
        #[cxx_name = "audioGainChanged"]
        pub unsafe fn audio_gain_changed(self: Pin<&mut PortGui>, audio_gain: f64);

        #[qsignal]
        #[cxx_name = "initializedChanged"]
        pub unsafe fn initialized_changed(self: Pin<&mut PortGui>, initialized: bool);

        #[qsignal]
        #[cxx_name = "nameChanged"]
        pub unsafe fn name_changed(self: Pin<&mut PortGui>, name: QString);

        #[qsignal]
        #[cxx_name = "mutedChanged"]
        pub unsafe fn muted_changed(self: Pin<&mut PortGui>, muted: bool);

        #[qsignal]
        #[cxx_name = "passthroughMutedChanged"]
        pub unsafe fn passthrough_muted_changed(self: Pin<&mut PortGui>, passthrough_muted: bool);

        #[qsignal]
        #[cxx_name = "audioInputPeakChanged"]
        pub unsafe fn audio_input_peak_changed(self: Pin<&mut PortGui>, audio_input_peak: f64);

        #[qsignal]
        #[cxx_name = "audioOutputPeakChanged"]
        pub unsafe fn audio_output_peak_changed(self: Pin<&mut PortGui>, audio_output_peak: f64);

        #[qsignal]
        #[cxx_name = "midiNInputEventsChanged"]
        pub unsafe fn midi_n_input_events_changed(
            self: Pin<&mut PortGui>,
            midi_n_input_events: i32,
        );

        #[qsignal]
        #[cxx_name = "midiNOutputEventsChanged"]
        pub unsafe fn midi_n_output_events_changed(
            self: Pin<&mut PortGui>,
            midi_n_output_events: i32,
        );

        #[qsignal]
        #[cxx_name = "midiNInputNotesActiveChanged"]
        pub unsafe fn midi_n_input_notes_active_changed(
            self: Pin<&mut PortGui>,
            midi_n_input_notes_active: i32,
        );

        #[qsignal]
        #[cxx_name = "midiNOutputNotesActiveChanged"]
        pub unsafe fn midi_n_output_notes_active_changed(
            self: Pin<&mut PortGui>,
            midi_n_output_notes_active: i32,
        );

        #[qsignal]
        pub unsafe fn backend_connect_external_port(self: Pin<&mut PortGui>, name: QString);

        #[qsignal]
        pub unsafe fn backend_disconnect_external_port(self: Pin<&mut PortGui>, name: QString);

        #[qsignal]
        pub unsafe fn backend_try_make_connections(
            self: Pin<&mut PortGui>,
            connections: QList_QString,
        );

        #[qsignal]
        pub unsafe fn backend_dummy_queue_audio_data(
            self: Pin<&mut PortGui>,
            audio_data: QList_f64,
        );

        #[qsignal]
        pub unsafe fn backend_dummy_request_data(self: Pin<&mut PortGui>, n: i32);

        #[qsignal]
        pub unsafe fn backend_dummy_queue_midi_msgs(
            self: Pin<&mut PortGui>,
            midi_msgs: QList_QVariant,
        );

        #[qsignal]
        pub unsafe fn backend_dummy_clear_queues(self: Pin<&mut PortGui>);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");

        #[rust_name = "qquickitem_from_ref_port_gui"]
        unsafe fn qquickitemFromRef(obj: &PortGui) -> &QQuickItem;

        #[rust_name = "qquickitem_from_ptr_port_gui"]
        unsafe fn qquickitemFromPtr(obj: *mut PortGui) -> *mut QQuickItem;
    }

    impl cxx_qt::Constructor<(*mut QQuickItem,), NewArguments = (*mut QQuickItem,)> for PortGui {}
}

use cxx_qt_lib_shoop::qquickitem::IsQQuickItem;
pub use ffi::PortGui;
use ffi::*;

pub struct PortGuiRust {
    // Properties
    pub initialized: bool,
    pub name: QString,
    pub muted: bool,
    pub passthrough_muted: bool,
    pub audio_input_peak: f64,
    pub audio_output_peak: f64,
    pub midi_n_input_events: i32,
    pub midi_n_output_events: i32,
    pub midi_n_input_notes_active: i32,
    pub midi_n_output_notes_active: i32,
    pub backend: *mut QObject,
    pub name_hint: QString,
    pub input_connectability: i32,
    pub output_connectability: i32,
    pub is_internal: bool,
    pub internal_port_connections: QList_QVariant,
    pub n_ringbuffer_samples: i32,
    pub audio_gain: f64,
}

impl Default for PortGuiRust {
    fn default() -> PortGuiRust {
        PortGuiRust {
            initialized: false,
            backend: std::ptr::null_mut(),
            name: QString::from(""),
            muted: false,
            passthrough_muted: false,
            audio_input_peak: 0.0,
            audio_output_peak: 0.0,
            midi_n_input_events: 0,
            midi_n_output_events: 0,
            midi_n_input_notes_active: 0,
            midi_n_output_notes_active: 0,
            name_hint: QString::from("unknown"),
            input_connectability: 0,
            output_connectability: 0,
            is_internal: false,
            internal_port_connections: QList_QVariant::default(),
            n_ringbuffer_samples: 0,
            audio_gain: 1.0,
        }
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

impl IsQQuickItem for PortGui {}

impl cxx_qt::Constructor<(*mut QQuickItem,)> for PortGui {
    type BaseArguments = (*mut QQuickItem,); // Will be passed to the base class constructor
    type InitializeArguments = (); // Will be passed to the "initialize" function
    type NewArguments = (*mut QQuickItem,); // Will be passed to the "new" function

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
