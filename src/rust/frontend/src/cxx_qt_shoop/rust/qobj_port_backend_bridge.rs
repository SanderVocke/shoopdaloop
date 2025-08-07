use common::logging::macros::*;
use crate::any_backend_port::AnyBackendPort;

shoop_log_unit!("Frontend.Port");

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qobject.h");
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
        // Backend -> Frontend properties
        #[qproperty(bool, initialized, READ=get_initialized, NOTIFY=initialized_changed)]
        #[qproperty(QString, name, READ=get_name, NOTIFY=name_changed)]
        #[qproperty(bool, muted, READ=get_muted, NOTIFY=muted_changed)]
        #[qproperty(bool, passthrough_muted, READ=get_passthrough_muted, NOTIFY=passthrough_muted_changed)]
        #[qproperty(f64, audio_input_peak, READ=get_audio_input_peak, NOTIFY=audio_input_peak_changed)]
        #[qproperty(f64, audio_output_peak, READ=get_audio_output_peak, NOTIFY=audio_output_peak_changed)]
        #[qproperty(i32, midi_n_input_events, READ=get_midi_n_input_events, NOTIFY=midi_n_input_events_changed)]
        #[qproperty(i32, midi_n_output_events, READ=get_midi_n_output_events, NOTIFY=midi_n_output_events_changed)]
        #[qproperty(i32, midi_n_input_notes_active, READ=get_midi_n_input_notes_active, NOTIFY=midi_n_input_notes_active_changed)]
        #[qproperty(i32, midi_n_output_notes_active, READ=get_midi_n_output_notes_active, NOTIFY=midi_n_output_notes_active_changed)]
        // Frontend -> Backend properties
        #[qproperty(bool, is_midi, READ, WRITE=set_is_midi, NOTIFY=is_midi_changed)]
        #[qproperty(*mut QObject, backend, READ, WRITE=set_backend, NOTIFY=backend_changed)]
        #[qproperty(*mut QObject, maybe_fx_chain, READ, WRITE=set_fx_chain, NOTIFY=fx_chain_changed)]
        #[qproperty(i32, fx_chain_port_idx, READ, WRITE=set_fx_chain_port_idx, NOTIFY=fx_chain_port_idx_changed)]
        #[qproperty(QString, name_hint, READ, WRITE=set_name_hint, NOTIFY=name_hint_changed)]
        #[qproperty(i32, input_connectability, READ, WRITE=set_input_connectability, NOTIFY=input_connectability_changed)]
        #[qproperty(i32, output_connectability, READ, WRITE=set_output_connectability, NOTIFY=output_connectability_changed)]
        #[qproperty(bool, is_internal, READ, WRITE=set_is_internal, NOTIFY=is_internal_changed)]
        #[qproperty(QList_QVariant, internal_port_connections, READ, WRITE=set_internal_port_connections, NOTIFY=internal_port_connections_changed)]
        #[qproperty(i32, n_ringbuffer_samples, READ, WRITE=set_n_ringbuffer_samples, NOTIFY=n_ringbuffer_samples_changed)]
        #[qproperty(f64, audio_gain, READ, WRITE=set_audio_gain, NOTIFY=audio_gain_changed)]
        // Other properties
        //#[qproperty(*mut QObject, backend_port_wrapper, READ=get_backend_port_wrapper)]
        type PortBackend = super::PortBackendRust;

        pub fn initialize_impl(self: Pin<&mut PortBackend>);

        #[qinvokable]
        pub fn get_initialized(self: &PortBackend) -> bool;
        #[qinvokable]
        pub fn get_name(self: &PortBackend) -> &QString;
        #[qinvokable]
        pub fn get_muted(self: &PortBackend) -> bool;
        #[qinvokable]
        pub fn get_passthrough_muted(self: &PortBackend) -> bool;
        #[qinvokable]
        pub fn get_audio_input_peak(self: &PortBackend) -> f64;
        #[qinvokable]
        pub fn get_audio_output_peak(self: &PortBackend) -> f64;
        #[qinvokable]
        pub fn get_midi_n_input_events(self: &PortBackend) -> i32;
        #[qinvokable]
        pub fn get_midi_n_output_events(self: &PortBackend) -> i32;
        #[qinvokable]
        pub fn get_midi_n_input_notes_active(self: &PortBackend) -> i32;
        #[qinvokable]
        pub fn get_midi_n_output_notes_active(self: &PortBackend) -> i32;

        #[qinvokable]
        pub fn connect_external_port(self: Pin<&mut PortBackend>, name: QString);

        #[qinvokable]
        pub fn disconnect_external_port(self: Pin<&mut PortBackend>, name: QString);

        #[qinvokable]
        pub fn get_connections_state(self: Pin<&mut PortBackend>) -> QMap_QString_QVariant;

        #[qinvokable]
        pub fn get_connected_external_ports(self: Pin<&mut PortBackend>) -> QList_QString;

        #[qinvokable]
        pub fn try_make_connections(self: Pin<&mut PortBackend>, connections: QList_QString);

        #[qinvokable]
        pub unsafe fn set_backend(self: Pin<&mut PortBackend>, backend: *mut QObject);

        #[qinvokable]
        pub fn set_name_hint(self: Pin<&mut PortBackend>, name_hint: QString);

        #[qinvokable]
        pub fn set_input_connectability(self: Pin<&mut PortBackend>, input_connectability: i32);

        #[qinvokable]
        pub fn set_output_connectability(self: Pin<&mut PortBackend>, output_connectability: i32);

        #[qinvokable]
        pub fn set_is_internal(self: Pin<&mut PortBackend>, is_internal: bool);

        #[qinvokable]
        pub unsafe fn set_fx_chain(self: Pin<&mut PortBackend>, fx_chain: *mut QObject);

        #[qinvokable]
        pub fn set_fx_chain_port_idx(self: Pin<&mut PortBackend>, fx_chain_port_idx: i32);

        #[qinvokable]
        pub fn set_internal_port_connections(
            self: Pin<&mut PortBackend>,
            internal_port_connections: QList_QVariant,
        );

        #[qinvokable]
        pub fn set_n_ringbuffer_samples(self: Pin<&mut PortBackend>, n_ringbuffer_samples: i32);

        #[qinvokable]
        pub fn set_audio_gain(self: Pin<&mut PortBackend>, audio_gain: f64);

        #[qinvokable]
        pub fn set_is_midi(self: Pin<&mut PortBackend>, is_midi: bool);

        #[qinvokable]
        pub fn dummy_queue_audio_data(self: Pin<&mut PortBackend>, audio_data: QList_f64);

        #[qinvokable]
        pub fn dummy_dequeue_audio_data(self: Pin<&mut PortBackend>) -> QList_f64;

        #[qinvokable]
        pub fn dummy_request_data(self: Pin<&mut PortBackend>, n: i32);

        #[qinvokable]
        pub fn dummy_queue_midi_msgs(self: Pin<&mut PortBackend>, midi_msgs: QList_QVariant);

        #[qinvokable]
        pub fn dummy_dequeue_midi_msgs(self: Pin<&mut PortBackend>) -> QList_QVariant;

        #[qinvokable]
        pub fn dummy_clear_queues(self: Pin<&mut PortBackend>);

        #[qsignal]
        #[cxx_name = "backendChanged"]
        pub unsafe fn backend_changed(self: Pin<&mut PortBackend>, backend: *mut QObject);

        #[qsignal]
        #[cxx_name = "nameHintChanged"]
        pub unsafe fn name_hint_changed(self: Pin<&mut PortBackend>, name_hint: QString);

        #[qsignal]
        #[cxx_name = "inputConnectabilityChanged"]
        pub unsafe fn input_connectability_changed(
            self: Pin<&mut PortBackend>,
            input_connectability: i32,
        );

        #[qsignal]
        #[cxx_name = "outputConnectabilityChanged"]
        pub unsafe fn output_connectability_changed(
            self: Pin<&mut PortBackend>,
            output_connectability: i32,
        );

        #[qsignal]
        #[cxx_name = "isInternalChanged"]
        pub unsafe fn is_internal_changed(self: Pin<&mut PortBackend>, is_internal: bool);

        #[qsignal]
        #[cxx_name = "internalPortConnectionsChanged"]
        pub unsafe fn internal_port_connections_changed(
            self: Pin<&mut PortBackend>,
            internal_port_connections: QList_QVariant,
        );

        #[qsignal]
        #[cxx_name = "nRingbufferSamplesChanged"]
        pub unsafe fn n_ringbuffer_samples_changed(
            self: Pin<&mut PortBackend>,
            n_ringbuffer_samples: i32,
        );

        #[qsignal]
        #[cxx_name = "audioGainChanged"]
        pub unsafe fn audio_gain_changed(self: Pin<&mut PortBackend>, audio_gain: f64);

        #[qsignal]
        #[cxx_name = "initializedChanged"]
        pub unsafe fn initialized_changed(self: Pin<&mut PortBackend>, initialized: bool);

        #[qsignal]
        #[cxx_name = "nameChanged"]
        pub unsafe fn name_changed(self: Pin<&mut PortBackend>, name: QString);

        #[qsignal]
        #[cxx_name = "mutedChanged"]
        pub unsafe fn muted_changed(self: Pin<&mut PortBackend>, muted: bool);

        #[qsignal]
        #[cxx_name = "passthroughMutedChanged"]
        pub unsafe fn passthrough_muted_changed(self: Pin<&mut PortBackend>, passthrough_muted: bool);

        #[qsignal]
        #[cxx_name = "audioInputPeakChanged"]
        pub unsafe fn audio_input_peak_changed(self: Pin<&mut PortBackend>, audio_input_peak: f64);

        #[qsignal]
        #[cxx_name = "audioOutputPeakChanged"]
        pub unsafe fn audio_output_peak_changed(self: Pin<&mut PortBackend>, audio_output_peak: f64);

        #[qsignal]
        #[cxx_name = "midiNInputEventsChanged"]
        pub unsafe fn midi_n_input_events_changed(
            self: Pin<&mut PortBackend>,
            midi_n_input_events: i32,
        );

        #[qsignal]
        #[cxx_name = "fxChainChanged"]
        pub unsafe fn fx_chain_changed(self: Pin<&mut PortBackend>, fx_chain: *mut QObject);

        #[qsignal]
        #[cxx_name = "fxChainPortIdxChanged"]
        pub unsafe fn fx_chain_port_idx_changed(self: Pin<&mut PortBackend>, fx_chain_port_idx: i32);

        #[qsignal]
        #[cxx_name = "midiNOutputEventsChanged"]
        pub unsafe fn midi_n_output_events_changed(
            self: Pin<&mut PortBackend>,
            midi_n_output_events: i32,
        );

        #[qsignal]
        #[cxx_name = "midiNInputNotesActiveChanged"]
        pub unsafe fn midi_n_input_notes_active_changed(
            self: Pin<&mut PortBackend>,
            midi_n_input_notes_active: i32,
        );

        #[qsignal]
        #[cxx_name = "midiNOutputNotesActiveChanged"]
        pub unsafe fn midi_n_output_notes_active_changed(
            self: Pin<&mut PortBackend>,
            midi_n_output_notes_active: i32,
        );

        #[qsignal]
        #[cxx_name = "isMidiChanged"]
        pub unsafe fn is_midi_changed(
            self: Pin<&mut PortBackend>,
            is_midi: QVariant
        );
    }
}

pub use ffi::PortBackend;
use ffi::*;

pub struct PortBackendRust {
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
    pub is_midi: bool,
    pub maybe_fx_chain: *mut QObject,
    pub fx_chain_port_idx: i32,

    // Other fields
    pub maybe_backend_port : Option<AnyBackendPort>,
}

impl Default for PortBackendRust {
    fn default() -> PortBackendRust {
        PortBackendRust {
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
            is_midi: false,
            maybe_fx_chain: std::ptr::null_mut(),
            fx_chain_port_idx: 0,
            maybe_backend_port: None,
        }
    }
}