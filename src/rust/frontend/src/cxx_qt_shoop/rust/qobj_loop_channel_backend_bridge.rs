use backend_bindings::PortDataType;
use common::logging::macros::*;
use cxx_qt_lib_shoop::{qobject::AsQObject, qweakpointer_qobject::QWeakPointer_QObject};

shoop_log_unit!("Frontend.LoopChannel");

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
        type QList_f32 = cxx_qt_lib::QList<f32>;
        type QList_QString = cxx_qt_lib::QList<cxx_qt_lib::QString>;

        include!("cxx-qt-lib/qmap.h");
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;
    }

    unsafe extern "RustQt" {
        #[qobject]
        // Backend -> Frontend properties
        #[qproperty(bool, initialized, READ=get_initialized, NOTIFY=initialized_changed)]
        // Frontend -> Backend properties
        #[qproperty(bool, is_midi, READ=get_is_midi, WRITE=set_is_midi, NOTIFY=is_midi_changed)]
        #[qproperty(*mut QObject, backend, READ, WRITE=set_backend, NOTIFY=backend_changed)]
        // Other properties
        #[qproperty(*mut QObject, channel_loop, READ=get_channel_loop, NOTIFY=channel_loop_changed)]
        type LoopChannelBackend = super::LoopChannelBackendRust;

        pub fn initialize_impl(self: Pin<&mut LoopChannelBackend>);

        #[qinvokable]
        pub fn get_initialized(self: &LoopChannelBackend) -> bool;

        #[qinvokable]
        pub unsafe fn set_backend(self: Pin<&mut LoopChannelBackend>, backend: *mut QObject);

        #[qinvokable]
        pub fn set_is_midi(self: Pin<&mut LoopChannelBackend>, is_midi: bool);

        #[qinvokable]
        pub fn set_channel_loop(self: Pin<&mut LoopChannelBackend>, parent_sharedptr: QVariant);

        #[qinvokable]
        pub fn get_is_midi(self: Pin<&mut LoopChannelBackend>) -> bool;

        #[qinvokable]
        pub fn get_channel_loop(self: Pin<&mut LoopChannelBackend>) -> *mut QObject;

        #[qinvokable]
        pub fn update(self: Pin<&mut LoopChannelBackend>);

        #[qinvokable]
        pub fn maybe_initialize_backend(self: Pin<&mut LoopChannelBackend>) -> bool;

        #[qinvokable]
        pub fn push_mode(self: Pin<&mut LoopChannelBackend>, mode: i32);

        #[qinvokable]
        pub fn push_audio_gain(self: Pin<&mut LoopChannelBackend>, audio_gain: f32);

        #[qinvokable]
        pub fn push_n_preplay_samples(self: Pin<&mut LoopChannelBackend>, n_preplay_samples: i32);

        #[qinvokable]
        pub fn push_start_offset(self: Pin<&mut LoopChannelBackend>, start_offset: i32);

        #[qinvokable]
        pub fn set_ports_to_connect(
            self: Pin<&mut LoopChannelBackend>,
            ports_to_connect: QList_QVariant,
        );

        #[qinvokable]
        pub fn load_audio_data(self: Pin<&mut LoopChannelBackend>, data: QList_f32);

        #[qinvokable]
        pub fn load_midi_data(self: Pin<&mut LoopChannelBackend>, data: QList_QVariant);

        #[qinvokable]
        pub fn get_audio_data(self: Pin<&mut LoopChannelBackend>) -> QList_f32;

        #[qinvokable]
        pub fn get_midi_data(self: Pin<&mut LoopChannelBackend>) -> QList_QVariant;

        #[qsignal]
        pub unsafe fn state_changed(
            self: Pin<&mut LoopChannelBackend>,
            initialized: bool,
            mode: i32,
            length: i32,
            start_offset: i32,
            played_back_sample: QVariant,
            n_preplay_samples: i32,
            data_dirty: bool,
            audio_gain: f32,
            output_peak: f32,
            n_events_triggered: i32,
            n_notes_active: i32,
        );

        #[qsignal]
        pub unsafe fn backend_changed(self: Pin<&mut LoopChannelBackend>, backend: *mut QObject);

        #[qsignal]
        pub unsafe fn initialized_changed(self: Pin<&mut LoopChannelBackend>, initialized: bool);

        #[qsignal]
        pub unsafe fn is_midi_changed(self: Pin<&mut LoopChannelBackend>, is_midi: bool);

        #[qsignal]
        pub unsafe fn channel_loop_changed(
            self: Pin<&mut LoopChannelBackend>,
            channel_loop: *mut QObject,
        );

        #[qsignal]
        pub unsafe fn mode_changed(self: Pin<&mut LoopChannelBackend>, mode: i32);

        #[qsignal]
        pub unsafe fn data_length_changed(self: Pin<&mut LoopChannelBackend>, length: i32);

        #[qsignal]
        pub unsafe fn start_offset_changed(self: Pin<&mut LoopChannelBackend>, start_offset: i32);

        #[qsignal]
        pub unsafe fn last_played_sample_changed(
            self: Pin<&mut LoopChannelBackend>,
            played_back_sample: QVariant,
        );

        #[qsignal]
        pub unsafe fn n_preplay_samples_changed(
            self: Pin<&mut LoopChannelBackend>,
            n_preplay_samples: i32,
        );

        #[qsignal]
        pub unsafe fn data_dirty_changed(self: Pin<&mut LoopChannelBackend>, data_dirty: bool);

        #[qsignal]
        pub unsafe fn audio_gain_changed(self: Pin<&mut LoopChannelBackend>, audio_gain: f32);

        #[qsignal]
        pub unsafe fn audio_output_peak_changed(
            self: Pin<&mut LoopChannelBackend>,
            output_peak: f32,
        );

        #[qsignal]
        pub unsafe fn midi_n_events_triggered_changed(
            self: Pin<&mut LoopChannelBackend>,
            n_events_triggered: i32,
        );

        #[qsignal]
        pub unsafe fn midi_n_notes_active_changed(
            self: Pin<&mut LoopChannelBackend>,
            n_notes_active: i32,
        );
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qobject.h");

        #[rust_name = "from_qobject_ref_loop_channel_backend"]
        unsafe fn fromQObjectRef(obj: &QObject, output: *mut *const LoopChannelBackend);

        #[rust_name = "from_qobject_mut_loop_channel_backend"]
        unsafe fn fromQObjectMut(obj: Pin<&mut QObject>, output: *mut *mut LoopChannelBackend);

        include!("cxx-qt-lib-shoop/make_raw.h");
        #[rust_name = "make_raw_loop_channel_backend"]
        unsafe fn make_raw() -> *mut LoopChannelBackend;

        include!("cxx-qt-lib-shoop/qobject.h");
        #[rust_name = "loop_channel_backend_qobject_from_ptr"]
        unsafe fn qobjectFromPtr(obj: *mut LoopChannelBackend) -> *mut QObject;

        #[rust_name = "loop_channel_backend_qobject_from_ref"]
        fn qobjectFromRef(obj: &LoopChannelBackend) -> &QObject;
    }
}

pub use ffi::LoopChannelBackend;
use ffi::*;

use crate::any_backend_channel::{AnyBackendChannel, AnyBackendChannelState};

pub struct LoopChannelBackendRust {
    // Properties
    pub initialized: bool,
    pub backend: *mut QObject,

    // Other fields
    pub maybe_backend_channel: Option<AnyBackendChannel>,
    pub prev_state: AnyBackendChannelState,
    pub data_type: Option<PortDataType>,
    pub channel_loop: Option<cxx::UniquePtr<QWeakPointer_QObject>>,
    pub ports_to_connect: Vec<cxx::UniquePtr<QWeakPointer_QObject>>,
    pub ports_connected: Vec<cxx::UniquePtr<QWeakPointer_QObject>>,
}

impl Default for LoopChannelBackendRust {
    fn default() -> LoopChannelBackendRust {
        LoopChannelBackendRust {
            initialized: false,
            backend: std::ptr::null_mut(),
            maybe_backend_channel: None,
            prev_state: AnyBackendChannelState::default(),
            data_type: None,
            channel_loop: None,
            ports_to_connect: Vec::new(),
            ports_connected: Vec::new(),
        }
    }
}

impl AsQObject for ffi::LoopChannelBackend {
    unsafe fn mut_qobject_ptr(&mut self) -> *mut ffi::QObject {
        ffi::loop_channel_backend_qobject_from_ptr(self as *mut Self)
    }

    unsafe fn ref_qobject_ptr(&self) -> *const ffi::QObject {
        ffi::loop_channel_backend_qobject_from_ref(self) as *const ffi::QObject
    }
}

impl cxx_qt_lib_shoop::qobject::FromQObject for LoopChannelBackend {
    unsafe fn ptr_from_qobject_ref(obj: &cxx_qt_lib_shoop::qobject::QObject) -> *const Self {
        let mut output: *const Self = std::ptr::null();
        from_qobject_ref_loop_channel_backend(obj, &mut output as *mut *const Self);
        output
    }

    unsafe fn ptr_from_qobject_mut(
        obj: std::pin::Pin<&mut cxx_qt_lib_shoop::qobject::QObject>,
    ) -> *mut Self {
        let mut output: *mut Self = std::ptr::null_mut();
        from_qobject_mut_loop_channel_backend(obj, &mut output as *mut *mut Self);
        output
    }
}
