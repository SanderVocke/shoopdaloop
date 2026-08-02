use common::logging::macros::*;
use cxx_qt_lib_shoop::qpointer::QPointerQObject;
use shoop_engine::PortDataType;

shoop_log_unit!("Frontend.LoopChannel");

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

        include!("cxx-qt-lib/qvector.h");
        type QVector_QVariant = cxx_qt_lib::QVector<cxx_qt_lib::QVariant>;
        type QVector_f32 = cxx_qt_lib::QVector<f32>;
        type QVector_QString = cxx_qt_lib::QVector<cxx_qt_lib::QString>;

        include!("cxx-qt-lib/qmap.h");
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QQuickItem]
        #[qproperty(bool, initialized, READ=get_initialized, NOTIFY=initialized_changed)]
        #[qproperty(i32, mode, READ=get_mode, NOTIFY=mode_changed)]
        #[qproperty(i32, data_length, READ=get_data_length, NOTIFY=data_length_changed)]
        #[qproperty(i32, start_offset, READ=get_start_offset, NOTIFY=start_offset_changed)]
        #[qproperty(i32, n_preplay_samples, READ=get_n_preplay_samples, NOTIFY=n_preplay_samples_changed)]
        #[qproperty(bool, data_dirty, READ=get_data_dirty, NOTIFY=data_dirty_changed)]
        #[qproperty(QVariant, last_played_sample, READ=get_last_played_sample, NOTIFY=last_played_sample_changed)]
        #[qproperty(QList_QVariant, connected_ports, READ=get_connected_ports, NOTIFY=connected_ports_changed)]
        #[qproperty(f32, audio_gain, READ=get_audio_gain, NOTIFY=audio_gain_changed)]
        #[qproperty(f32, audio_output_peak, READ=get_audio_output_peak, NOTIFY=audio_output_peak_changed)]
        #[qproperty(i32, midi_n_events_triggered, READ=get_midi_n_events_triggered, NOTIFY=midi_n_events_triggered_changed)]
        #[qproperty(i32, midi_n_notes_active, READ=get_midi_n_notes_active, NOTIFY=midi_n_notes_active_changed)]
        // Frontend -> Backend properties
        #[qproperty(bool, is_midi, READ=get_is_midi, WRITE=set_is_midi, NOTIFY=is_midi_changed)]
        #[qproperty(*mut QObject, backend, READ, WRITE=set_backend, NOTIFY=backend_changed)]
        #[qproperty(QList_QVariant, ports_to_connect, READ, WRITE=set_ports_to_connect, NOTIFY=ports_to_connect_changed)]
        #[qproperty(*mut QObject, channel_loop, READ=get_channel_loop, WRITE=set_channel_loop, NOTIFY=channel_loop_changed)]
        #[qproperty(QString, instance_identifier, READ, WRITE=set_instance_identifier, NOTIFY=instance_identifier_changed)]
        #[qproperty(QVariant, recording_started_at, READ, WRITE, NOTIFY)]
        type LoopChannelGui = super::LoopChannelGuiRust;

        pub fn initialize_impl(self: Pin<&mut LoopChannelGui>);

        #[qinvokable]
        pub fn get_initialized(self: &LoopChannelGui) -> bool;

        #[qinvokable]
        pub unsafe fn set_backend(self: Pin<&mut LoopChannelGui>, backend: *mut QObject);

        #[qinvokable]
        pub fn set_is_midi(self: Pin<&mut LoopChannelGui>, is_midi: bool);

        #[qinvokable]
        pub unsafe fn set_channel_loop(self: Pin<&mut LoopChannelGui>, channel_loop: *mut QObject);

        #[qinvokable]
        pub fn get_is_midi(self: Pin<&mut LoopChannelGui>) -> bool;

        #[qinvokable]
        pub fn get_channel_loop(self: Pin<&mut LoopChannelGui>) -> *mut QObject;

        #[qinvokable]
        pub fn update(self: Pin<&mut LoopChannelGui>);

        #[qinvokable]
        pub fn maybe_initialize_backend(self: Pin<&mut LoopChannelGui>) -> bool;

        #[qinvokable]
        pub fn push_mode(self: Pin<&mut LoopChannelGui>, mode: i32);

        #[qinvokable]
        pub fn push_audio_gain(self: Pin<&mut LoopChannelGui>, audio_gain: f32);

        #[qinvokable]
        pub fn push_n_preplay_samples(self: Pin<&mut LoopChannelGui>, n_preplay_samples: i32);

        #[qinvokable]
        pub fn push_start_offset(self: Pin<&mut LoopChannelGui>, start_offset: i32);

        #[qinvokable]
        pub fn set_ports_to_connect(
            self: Pin<&mut LoopChannelGui>,
            ports_to_connect: QList_QVariant,
        );

        #[qinvokable]
        pub fn load_audio_data(self: Pin<&mut LoopChannelGui>, data: QVector_f32);

        #[qinvokable]
        pub fn load_midi_data(self: Pin<&mut LoopChannelGui>, data: QVector_QVariant);

        #[qinvokable]
        pub fn get_audio_data(self: Pin<&mut LoopChannelGui>) -> QVector_f32;

        #[qinvokable]
        pub fn get_midi_data(self: Pin<&mut LoopChannelGui>) -> QVector_QVariant;

        #[qinvokable]
        pub fn get_data(self: Pin<&mut LoopChannelGui>) -> QVector_QVariant;

        #[qinvokable]
        pub fn get_data_length(self: Pin<&mut LoopChannelGui>) -> i32;

        #[qinvokable]
        pub fn update_port_connections(self: Pin<&mut LoopChannelGui>);

        #[qinvokable]
        pub fn reset_state_tracking(self: Pin<&mut LoopChannelGui>);

        #[qinvokable]
        pub fn clear_data_dirty(self: Pin<&mut LoopChannelGui>);

        #[qinvokable]
        pub fn get_connected_ports(self: Pin<&mut LoopChannelGui>) -> QList_QVariant; // Weak pointers

        #[qinvokable]
        pub fn set_instance_identifier(
            self: Pin<&mut LoopChannelGui>,
            instance_identifier: QString,
        );

        #[qinvokable]
        pub fn clear(self: Pin<&mut LoopChannelGui>, length: i32);

        #[qinvokable]
        pub fn get_data_async_and_send_to(
            self: Pin<&mut LoopChannelGui>,
            send_to_object: *mut QObject,
            method_signature: QString,
        ) -> *mut QObject;

        #[qinvokable]
        pub fn deinit(self: Pin<&mut LoopChannelGui>);

        #[qinvokable]
        pub fn get_mode(self: &LoopChannelGui) -> i32;
        #[qinvokable]
        pub fn get_start_offset(self: &LoopChannelGui) -> i32;
        #[qinvokable]
        pub fn get_n_preplay_samples(self: &LoopChannelGui) -> i32;
        #[qinvokable]
        pub fn get_data_dirty(self: &LoopChannelGui) -> bool;
        #[qinvokable]
        pub fn get_last_played_sample(self: &LoopChannelGui) -> QVariant;
        #[qinvokable]
        pub fn get_audio_gain(self: &LoopChannelGui) -> f32;
        #[qinvokable]
        pub fn get_audio_output_peak(self: &LoopChannelGui) -> f32;
        #[qinvokable]
        pub fn get_midi_n_events_triggered(self: &LoopChannelGui) -> i32;
        #[qinvokable]
        pub fn get_midi_n_notes_active(self: &LoopChannelGui) -> i32;

        #[qsignal]
        pub unsafe fn ports_to_connect_changed(
            self: Pin<&mut LoopChannelGui>,
            ports: QList_QVariant,
        );

        #[qsignal]
        pub unsafe fn state_changed(
            self: Pin<&mut LoopChannelGui>,
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
        pub unsafe fn backend_changed(self: Pin<&mut LoopChannelGui>, backend: *mut QObject);

        #[qsignal]
        pub unsafe fn initialized_changed(self: Pin<&mut LoopChannelGui>, initialized: bool);

        #[qsignal]
        pub unsafe fn is_midi_changed(self: Pin<&mut LoopChannelGui>, is_midi: bool);

        #[qsignal]
        pub unsafe fn channel_loop_changed(
            self: Pin<&mut LoopChannelGui>,
            channel_loop: *mut QObject,
        );

        #[qsignal]
        pub unsafe fn mode_changed(self: Pin<&mut LoopChannelGui>, mode: i32);

        #[qsignal]
        pub unsafe fn data_length_changed(self: Pin<&mut LoopChannelGui>, length: i32);

        #[qsignal]
        pub unsafe fn start_offset_changed(self: Pin<&mut LoopChannelGui>, start_offset: i32);

        #[qsignal]
        pub unsafe fn connected_ports_changed(
            self: Pin<&mut LoopChannelGui>,
            connected_ports: QList_QVariant,
        );

        #[qsignal]
        pub unsafe fn last_played_sample_changed(
            self: Pin<&mut LoopChannelGui>,
            played_back_sample: QVariant,
        );

        #[qsignal]
        pub unsafe fn n_preplay_samples_changed(
            self: Pin<&mut LoopChannelGui>,
            n_preplay_samples: i32,
        );

        #[qsignal]
        pub unsafe fn data_dirty_changed(self: Pin<&mut LoopChannelGui>, data_dirty: bool);

        #[qsignal]
        pub unsafe fn audio_gain_changed(self: Pin<&mut LoopChannelGui>, audio_gain: f32);

        #[qsignal]
        pub unsafe fn audio_output_peak_changed(self: Pin<&mut LoopChannelGui>, output_peak: f32);

        #[qsignal]
        pub unsafe fn midi_n_events_triggered_changed(
            self: Pin<&mut LoopChannelGui>,
            n_events_triggered: i32,
        );

        #[qsignal]
        pub unsafe fn midi_n_notes_active_changed(
            self: Pin<&mut LoopChannelGui>,
            n_notes_active: i32,
        );

        #[qsignal]
        pub unsafe fn instance_identifier_changed(
            self: Pin<&mut LoopChannelGui>,
            instance_identifier: QString,
        );
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qobject.h");
        include!("cxx-qt-lib-shoop/qquickitem.h");

        #[rust_name = "qquickitem_from_ref_loop_channel_gui"]
        unsafe fn qquickitemFromRef(obj: &LoopChannelGui) -> &QQuickItem;

        #[rust_name = "qquickitem_from_ptr_loop_channel_gui"]
        unsafe fn qquickitemFromPtr(obj: *mut LoopChannelGui) -> *mut QQuickItem;

        #[rust_name = "from_qobject_ref_loop_channel_gui"]
        unsafe fn fromQObjectRef(obj: &QObject, output: *mut *const LoopChannelGui);

        #[rust_name = "from_qobject_mut_loop_channel_gui"]
        unsafe fn fromQObjectMut(obj: Pin<&mut QObject>, output: *mut *mut LoopChannelGui);

        include!("cxx-qt-lib-shoop/register_qml_type.h");
        #[rust_name = "register_qml_type_loop_channel_gui"]
        unsafe fn register_qml_type(
            inference_example: *mut LoopChannelGui,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );

        #[rust_name = "loop_channel_gui_qobject_from_ptr"]
        unsafe fn qobjectFromPtr(obj: *mut LoopChannelGui) -> *mut QObject;

        #[rust_name = "loop_channel_gui_qobject_from_ref"]
        fn qobjectFromRef(obj: &LoopChannelGui) -> &QObject;
    }

    impl cxx_qt::Constructor<(*mut QQuickItem,), NewArguments = (*mut QQuickItem,)> for LoopChannelGui {}
    impl cxx_qt::Constructor<()> for LoopChannelGui {}
}

pub use ffi::LoopChannelGui;
use ffi::*;

use crate::any_backend_channel::{AnyBackendChannel, AnyBackendChannelState};

pub struct LoopChannelGuiRust {
    // Properties
    pub initialized: bool,
    pub backend: *mut QObject,
    pub recording_started_at: QVariant,

    // Other fields
    pub maybe_backend_channel: Option<AnyBackendChannel>,
    pub prev_state: AnyBackendChannelState,
    pub data_type: Option<PortDataType>,
    pub channel_loop: *mut QObject,
    pub channel_loop_guard: cxx::UniquePtr<QPointerQObject>,
    pub ports_to_connect: QList_QVariant,
    pub ports_connected: QList_QVariant,
    pub instance_identifier: QString,
}

impl Default for LoopChannelGuiRust {
    fn default() -> LoopChannelGuiRust {
        LoopChannelGuiRust {
            initialized: false,
            backend: std::ptr::null_mut(),
            maybe_backend_channel: None,
            prev_state: AnyBackendChannelState::default(),
            data_type: None,
            channel_loop: std::ptr::null_mut(),
            channel_loop_guard: cxx::UniquePtr::null(),
            ports_to_connect: QList_QVariant::default(),
            ports_connected: QList_QVariant::default(),
            instance_identifier: QString::from("unknown"),
            recording_started_at: QVariant::default(),
        }
    }
}

impl cxx_qt_lib_shoop::qobject::FromQObject for LoopChannelGui {
    unsafe fn ptr_from_qobject_ref(obj: &cxx_qt::QObject) -> *const Self {
        let mut output: *const Self = std::ptr::null();
        from_qobject_ref_loop_channel_gui(obj, &mut output as *mut *const Self);
        output
    }

    unsafe fn ptr_from_qobject_mut(obj: std::pin::Pin<&mut cxx_qt::QObject>) -> *mut Self {
        let mut output: *mut Self = std::ptr::null_mut();
        from_qobject_mut_loop_channel_gui(obj, &mut output as *mut *mut Self);
        output
    }
}

impl cxx_qt_lib_shoop::qquickitem::AsQQuickItem for LoopChannelGui {
    unsafe fn mut_qquickitem_ptr(&mut self) -> *mut QQuickItem {
        qquickitem_from_ptr_loop_channel_gui(self as *mut Self)
    }

    unsafe fn ref_qquickitem_ptr(&self) -> *const QQuickItem {
        qquickitem_from_ref_loop_channel_gui(self) as *const QQuickItem
    }
}

impl cxx_qt_lib_shoop::qquickitem::IsQQuickItem for LoopChannelGui {}

impl cxx_qt::Constructor<(*mut QQuickItem,)> for LoopChannelGui {
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

    fn new(_parent: (*mut QQuickItem,)) -> LoopChannelGuiRust {
        LoopChannelGuiRust::default()
    }

    fn initialize(self: std::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        LoopChannelGui::initialize_impl(self);
    }
}

impl cxx_qt::Constructor<()> for LoopChannelGui {
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

    fn new(_parent: ()) -> LoopChannelGuiRust {
        LoopChannelGuiRust::default()
    }

    fn initialize(self: std::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        LoopChannelGui::initialize_impl(self);
    }
}
