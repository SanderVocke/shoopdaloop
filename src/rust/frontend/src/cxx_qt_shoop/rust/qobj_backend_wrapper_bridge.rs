use common::logging::macros::*;
use shoop_engine::app_backend::AudioDriver;
use shoop_engine::app_backend::BackendSession;
use std::time;
shoop_log_unit!("Frontend.BackendWrapper");

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
        include!("cxx-qt-lib-shoop/qtimer.h");
        type QTimer = cxx_qt_lib_shoop::qtimer::QTimer;

        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qlist.h");
        type QList_QVariant = cxx_qt_lib::QList<cxx_qt_lib::QVariant>;

        include!("cxx-qt-lib/qmap.h");
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;

    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QQuickItem]
        #[qproperty(bool, ready)]
        #[qproperty(i32, update_interval_ms, READ=get_update_interval_ms, WRITE=set_update_interval_ms, NOTIFY=update_interval_ms_changed)]
        #[qproperty(i32, actual_backend_type)]
        #[qproperty(QString, client_name_hint)]
        #[qproperty(i32, backend_type)]
        #[qproperty(bool, backend_type_explicit)]
        #[qproperty(QString, init_error)]
        #[qproperty(i32, xruns)]
        #[qproperty(i32, stale_graph_cycles)]
        #[qproperty(i32, last_processed)]
        #[qproperty(f32, dsp_load)]
        #[qproperty(i32, n_audio_buffers_created)]
        #[qproperty(i32, n_audio_buffers_available)]
        #[qproperty(i32, sample_rate)]
        #[qproperty(i32, buffer_size)]
        #[qproperty(f32, last_update_interval)]
        #[qproperty(i32, refresh_epoch)]
        #[qproperty(QMap_QString_QVariant, driver_setting_overrides)]
        type BackendWrapper = super::BackendWrapperRust;

        #[qsignal]
        pub fn updated_on_gui_thread(self: Pin<&mut BackendWrapper>);

        #[qsignal]
        pub fn update_interval_ms_changed(self: Pin<&mut BackendWrapper>, update_interval_ms: i32);

        #[qinvokable]
        pub fn refresh(self: Pin<&mut BackendWrapper>);

        #[qinvokable]
        pub fn get_update_interval_ms(self: &BackendWrapper) -> i32;

        #[qinvokable]
        pub fn set_update_interval_ms(self: Pin<&mut BackendWrapper>, update_interval_ms: i32);

        #[qinvokable]
        pub fn close(self: Pin<&mut BackendWrapper>);

        #[qinvokable]
        pub fn dummy_enter_controlled_mode(self: Pin<&mut BackendWrapper>);

        #[qinvokable]
        pub fn dummy_enter_automatic_mode(self: Pin<&mut BackendWrapper>);

        #[qinvokable]
        pub fn dummy_is_controlled(self: Pin<&mut BackendWrapper>) -> bool;

        #[qinvokable]
        pub fn dummy_wait_controlled_mode(self: Pin<&mut BackendWrapper>);

        #[qinvokable]
        pub fn dummy_request_controlled_frames(self: Pin<&mut BackendWrapper>, _n: i32);

        #[qinvokable]
        pub fn dummy_n_requested_frames(self: Pin<&mut BackendWrapper>) -> i32;

        #[qinvokable]
        pub fn dummy_run_requested_frames(self: Pin<&mut BackendWrapper>);

        #[qinvokable]
        pub fn dummy_add_external_mock_port(
            self: Pin<&mut BackendWrapper>,
            _name: QString,
            _direction: i32,
            _data_type: i32,
        );

        #[qinvokable]
        pub fn dummy_remove_external_mock_port(self: Pin<&mut BackendWrapper>, _name: QString);

        #[qinvokable]
        pub fn dummy_remove_all_external_mock_ports(self: Pin<&mut BackendWrapper>);

        #[qinvokable]
        pub fn wait_process(self: Pin<&mut BackendWrapper>);

        #[qinvokable]
        pub fn maybe_init(self: Pin<&mut BackendWrapper>);

        #[qinvokable]
        pub fn get_profiling_report(self: Pin<&mut BackendWrapper>) -> QVariant;

        #[qinvokable]
        pub fn backend_type_is_supported(self: Pin<&mut BackendWrapper>, _type: i32) -> bool;

        #[qinvokable]
        pub fn allow_missing_backends(self: Pin<&mut BackendWrapper>) -> bool;

        #[qinvokable]
        pub fn segfault_on_process_thread(self: Pin<&mut BackendWrapper>);

        #[qinvokable]
        pub fn abort_on_process_thread(self: Pin<&mut BackendWrapper>);

        #[qinvokable]
        pub fn find_external_ports(
            self: Pin<&mut BackendWrapper>,
            _maybe_name_regex: QString,
            _port_direction: i32,
            _data_type: i32,
        ) -> QList_QVariant;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qobject.h");
        include!("cxx-qt-lib-shoop/qquickitem.h");

        #[rust_name = "qquickitem_from_ref_backend_wrapper"]
        unsafe fn qquickitemFromRef(obj: &BackendWrapper) -> &QQuickItem;

        #[rust_name = "qquickitem_from_ptr_backend_wrapper"]
        unsafe fn qquickitemFromPtr(obj: *mut BackendWrapper) -> *mut QQuickItem;

        #[rust_name = "from_qobject_ref_backend_wrapper"]
        unsafe fn fromQObjectRef(obj: &QObject, output: *mut *const BackendWrapper);

        #[rust_name = "from_qobject_mut_backend_wrapper"]
        unsafe fn fromQObjectMut(obj: Pin<&mut QObject>, output: *mut *mut BackendWrapper);

        include!("cxx-qt-lib-shoop/make_unique.h");
        #[rust_name = "make_unique_backend_wrapper"]
        fn make_unique() -> UniquePtr<BackendWrapper>;

        include!("cxx-qt-lib-shoop/qobject_classname.h");
        #[rust_name = "qobject_class_name_backend_wrapper"]
        fn qobject_class_name(obj: &BackendWrapper) -> Result<&str>;

        include!("cxx-qt-lib-shoop/register_qml_type.h");
        #[rust_name = "register_qml_type_backend_wrapper"]
        unsafe fn register_qml_type(
            inference_example: *mut BackendWrapper,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );
    }

    impl cxx_qt::Constructor<(*mut QQuickItem,), NewArguments = (*mut QQuickItem,)> for BackendWrapper {}
    impl cxx_qt::Constructor<(), NewArguments = ()> for BackendWrapper {}
}

use common::tracing_helpers::TracyPlotter;
pub use ffi::BackendWrapper;
use ffi::*;

pub struct BackendWrapperRust {
    // Properties
    ready: bool,
    pub update_interval_ms: i32,
    actual_backend_type: i32,
    client_name_hint: QString,
    backend_type: i32,
    backend_type_explicit: bool,
    init_error: QString,
    xruns: i32,
    stale_graph_cycles: i32,
    last_processed: i32,
    dsp_load: f32,
    driver_setting_overrides: QMap_QString_QVariant,
    n_audio_buffers_created: i32,
    n_audio_buffers_available: i32,
    last_update_interval: f32,
    refresh_epoch: i32,
    sample_rate: i32,
    buffer_size: i32,

    // Rust-side only
    pub driver: Option<AudioDriver>,
    pub session: Option<BackendSession>,
    pub closed: bool,
    pub last_updated: Option<time::Instant>,
    pub plotter_mode: TracyPlotter,
    pub plotter_samples_requested: TracyPlotter,
    pub plotter_samples_pending: TracyPlotter,
    pub plotter_ready: TracyPlotter,
    pub plotter_backend_type: TracyPlotter,
    pub plotter_xruns: TracyPlotter,
    pub plotter_stale_graph_cycles: TracyPlotter,
    pub plotter_dsp_load: TracyPlotter,
    pub plotter_last_processed: TracyPlotter,
    pub plotter_audio_buffers_created: TracyPlotter,
    pub plotter_audio_buffers_available: TracyPlotter,
    pub plotter_sample_rate: TracyPlotter,
    pub plotter_buffer_size: TracyPlotter,
    pub plotter_update_interval: TracyPlotter,
    pub plotter_cycles: TracyPlotter,
    pub plotter_frames: TracyPlotter,
    pub plotter_pending_commands: TracyPlotter,
    pub plotter_commands_applied: TracyPlotter,
    pub plotter_last_applied_command: TracyPlotter,
    pub plotter_trace_snapshots_dropped: TracyPlotter,
    pub plotter_capture_underruns: TracyPlotter,
    pub plotter_capture_overruns: TracyPlotter,
    pub plotter_graph_arms: TracyPlotter,
    pub plotter_graph_applies: TracyPlotter,
    pub plotter_callback_last_ns: TracyPlotter,
    pub plotter_callback_worst_ns: TracyPlotter,
    pub plotter_callback_budget_overruns: TracyPlotter,
    pub plotter_schedule_request_id: TracyPlotter,
    pub plotter_schedule_applied_id: TracyPlotter,
    pub plotter_stuck_cycles: TracyPlotter,
    pub plotter_stale_cycles: TracyPlotter,
    pub plotter_sub_blocks_last_cycle: TracyPlotter,
}

impl Default for BackendWrapperRust {
    fn default() -> BackendWrapperRust {
        BackendWrapperRust {
            // Properties
            ready: false,
            update_interval_ms: 50,
            actual_backend_type: 0,
            client_name_hint: QString::default(),
            backend_type: -1,
            backend_type_explicit: false,
            init_error: QString::default(),
            xruns: 0,
            stale_graph_cycles: 0,
            last_processed: 0,
            dsp_load: 0.0,
            driver_setting_overrides: QMap_QString_QVariant::default(),
            n_audio_buffers_available: 0,
            n_audio_buffers_created: 0,
            last_update_interval: 1.0,
            refresh_epoch: 0,
            sample_rate: 1,
            buffer_size: 1,

            // Rust-side only
            driver: None,
            session: None,
            closed: false,
            last_updated: None,
            plotter_mode: TracyPlotter::new("mode"),
            plotter_samples_requested: TracyPlotter::new("samples_requested"),
            plotter_samples_pending: TracyPlotter::new("samples_pending"),
            plotter_ready: TracyPlotter::new("ready"),
            plotter_backend_type: TracyPlotter::new("backend_type"),
            plotter_xruns: TracyPlotter::new("xruns"),
            plotter_stale_graph_cycles: TracyPlotter::new("stale_graph_cycles"),
            plotter_dsp_load: TracyPlotter::new("dsp_load_percent"),
            plotter_last_processed: TracyPlotter::new("last_processed"),
            plotter_audio_buffers_created: TracyPlotter::new("audio_buffers_created"),
            plotter_audio_buffers_available: TracyPlotter::new("audio_buffers_available"),
            plotter_sample_rate: TracyPlotter::new("sample_rate"),
            plotter_buffer_size: TracyPlotter::new("buffer_size"),
            plotter_update_interval: TracyPlotter::new("update_interval_ms"),
            plotter_cycles: TracyPlotter::new("cycles"),
            plotter_frames: TracyPlotter::new("frames"),
            plotter_pending_commands: TracyPlotter::new("pending_commands"),
            plotter_commands_applied: TracyPlotter::new("commands_applied"),
            plotter_last_applied_command: TracyPlotter::new("last_applied_command"),
            plotter_trace_snapshots_dropped: TracyPlotter::new("trace_snapshots_dropped"),
            plotter_capture_underruns: TracyPlotter::new("capture_underruns"),
            plotter_capture_overruns: TracyPlotter::new("capture_overruns"),
            plotter_graph_arms: TracyPlotter::new("graph_arms"),
            plotter_graph_applies: TracyPlotter::new("graph_applies"),
            plotter_callback_last_ns: TracyPlotter::new("callback_last_ns"),
            plotter_callback_worst_ns: TracyPlotter::new("callback_worst_ns"),
            plotter_callback_budget_overruns: TracyPlotter::new("callback_budget_overruns"),
            plotter_schedule_request_id: TracyPlotter::new("schedule_request_id"),
            plotter_schedule_applied_id: TracyPlotter::new("schedule_applied_id"),
            plotter_stuck_cycles: TracyPlotter::new("stuck_cycles"),
            plotter_stale_cycles: TracyPlotter::new("stale_cycles"),
            plotter_sub_blocks_last_cycle: TracyPlotter::new("sub_blocks_last_cycle"),
        }
    }
}

impl cxx_qt_lib_shoop::qquickitem::AsQQuickItem for BackendWrapper {
    unsafe fn mut_qquickitem_ptr(&mut self) -> *mut QQuickItem {
        qquickitem_from_ptr_backend_wrapper(self as *mut Self)
    }
    unsafe fn ref_qquickitem_ptr(&self) -> *const QQuickItem {
        qquickitem_from_ref_backend_wrapper(self) as *const QQuickItem
    }
}

impl cxx_qt_lib_shoop::qobject::FromQObject for BackendWrapper {
    unsafe fn ptr_from_qobject_ref(obj: &cxx_qt::QObject) -> *const Self {
        let mut output: *const Self = std::ptr::null();
        from_qobject_ref_backend_wrapper(obj, &mut output as *mut *const Self);
        output
    }

    unsafe fn ptr_from_qobject_mut(obj: std::pin::Pin<&mut cxx_qt::QObject>) -> *mut Self {
        let mut output: *mut Self = std::ptr::null_mut();
        from_qobject_mut_backend_wrapper(obj, &mut output as *mut *mut Self);
        output
    }
}

impl cxx_qt_lib_shoop::qquickitem::IsQQuickItem for BackendWrapper {}

impl cxx_qt::Constructor<(*mut QQuickItem,)> for BackendWrapper {
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

    fn new(_parent: (*mut QQuickItem,)) -> BackendWrapperRust {
        BackendWrapperRust::default()
    }
}

impl cxx_qt::Constructor<()> for BackendWrapper {
    type BaseArguments = (); // Will be passed to the base class constructor
    type InitializeArguments = (); // Will be passed to the "initialize" function
    type NewArguments = (); // Will be passed to the "new" function

    fn route_arguments(
        _args: (),
    ) -> (
        Self::NewArguments,
        Self::BaseArguments,
        Self::InitializeArguments,
    ) {
        ((), (), ())
    }

    fn new(_args: ()) -> BackendWrapperRust {
        BackendWrapperRust::default()
    }
}
