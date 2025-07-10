use backend_bindings::AudioDriver;
use backend_bindings::BackendSession;
use common::logging::macros::*;
use std::time;
shoop_log_unit!("Frontend.BackendWrapper");

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");
        type QQuickItem = crate::cxx_qt_lib_shoop::qquickitem::QQuickItem;

        include!("cxx-qt-lib-shoop/qthread.h");
        type QThread = crate::cxx_qt_lib_shoop::qthread::QThread;

        include!("cxx-qt-lib-shoop/qtimer.h");
        type QTimer = crate::cxx_qt_lib_shoop::qtimer::QTimer;

        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qlist.h");
        type QList_QVariant = cxx_qt_lib::QList<cxx_qt_lib::QVariant>;

        include!("cxx-qt-lib/qmap.h");
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;

        include!("cxx-qt-lib-shoop/qobject.h");
        type QObject = crate::cxx_qt_lib_shoop::qobject::QObject;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QQuickItem]
        #[qproperty(bool, ready)]
        #[qproperty(i32, update_interval_ms)]
        #[qproperty(i32, actual_backend_type)]
        #[qproperty(QString, client_name_hint)]
        #[qproperty(i32, backend_type)]
        #[qproperty(i32, xruns)]
        #[qproperty(i32, last_processed)]
        #[qproperty(f32, dsp_load)]
        #[qproperty(i32, n_audio_buffers_created)]
        #[qproperty(i32, n_audio_buffers_available)]
        #[qproperty(f32, last_update_interval)]
        #[qproperty(QMap_QString_QVariant, driver_setting_overrides)]
        type BackendWrapper = super::BackendWrapperRust;

        #[qsignal]
        pub fn updated_on_gui_thread(self: Pin<&mut BackendWrapper>);

        #[qsignal]
        pub fn updated_on_backend_thread(self: Pin<&mut BackendWrapper>);

        #[qinvokable]
        pub fn update_on_gui_thread(self: Pin<&mut BackendWrapper>);

        #[qinvokable]
        pub fn update_on_other_thread(self: Pin<&mut BackendWrapper>);

        #[qinvokable]
        pub fn get_sample_rate(self: Pin<&mut BackendWrapper>) -> i32;

        #[qinvokable]
        pub fn get_buffer_size(self: Pin<&mut BackendWrapper>) -> i32;

        #[qinvokable]
        pub fn get_gui_thread(self: &BackendWrapper) -> *mut QThread;

        #[qinvokable]
        pub fn get_backend_thread(self: &BackendWrapper) -> *mut QThread;

        #[qinvokable]
        pub fn close(self: Pin<&mut BackendWrapper>);

        #[qinvokable]
        pub fn dummy_enter_controlled_mode(self: Pin<&mut BackendWrapper>);

        #[qinvokable]
        pub fn dummy_enter_automatic_mode(self: Pin<&mut BackendWrapper>);

        #[qinvokable]
        pub fn dummy_is_controlled(self: Pin<&mut BackendWrapper>) -> bool;

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
        include!("cxx-qt-lib-shoop/qquickitem.h");

        #[rust_name = "qquickitem_from_ref_backend_wrapper"]
        unsafe fn qquickitemFromRef(obj: &BackendWrapper) -> &QQuickItem;

        #[rust_name = "qquickitem_from_ptr_backend_wrapper"]
        unsafe fn qquickitemFromPtr(obj: *mut BackendWrapper) -> *mut QQuickItem;

        include!("cxx-qt-shoop/make_unique.h");
        #[rust_name = "make_unique_backend_wrapper"]
        fn make_unique() -> UniquePtr<BackendWrapper>;

        include!("cxx-qt-shoop/qobject_classname.h");
        #[rust_name = "qobject_class_name_backend_wrapper"]
        fn qobject_class_name(obj: &BackendWrapper) -> Result<&str>;

        include!("cxx-qt-shoop/register_qml_type.h");
        #[rust_name = "register_qml_type_backend_wrapper"]
        fn register_qml_type(
            inference_example: &BackendWrapper,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );

        include!("cxx-qt-shoop/cast_ptr.h");
        #[rust_name = "qobject_ptr_to_backend_ptr"]
        unsafe fn cast_qobject_ptr(obj: *mut QObject) -> *mut BackendWrapper;
    }

    impl cxx_qt::Constructor<(*mut QQuickItem,), NewArguments = (*mut QQuickItem,)> for BackendWrapper {}
    impl cxx_qt::Constructor<(), NewArguments = ()> for BackendWrapper {}
}

pub use ffi::BackendWrapper;
use ffi::*;

#[derive(Copy, Clone)]
pub struct BackendWrapperUpdateData {
    pub xruns: i32,
    pub dsp_load: f32,
    pub last_processed: i32,
    pub n_audio_buffers_created: i32,
    pub n_audio_buffers_available: i32,
}
pub struct BackendWrapperRust {
    // Properties
    ready: bool,
    update_interval_ms: i32,
    actual_backend_type: i32,
    client_name_hint: QString,
    backend_type: i32,
    xruns: i32,
    last_processed: i32,
    dsp_load: f32,
    driver_setting_overrides: QMap_QString_QVariant,
    n_audio_buffers_created: i32,
    n_audio_buffers_available: i32,
    last_update_interval: f32,

    // Rust-side only
    pub driver: Option<AudioDriver>,
    pub session: Option<BackendSession>,
    pub update_data: Option<BackendWrapperUpdateData>,
    pub closed: bool,
    pub last_updated: Option<time::Instant>,
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
            xruns: 0,
            last_processed: 0,
            dsp_load: 0.0,
            driver_setting_overrides: QMap_QString_QVariant::default(),
            n_audio_buffers_available: 0,
            n_audio_buffers_created: 0,
            last_update_interval: 1.0,

            // Rust-side only
            driver: None,
            session: None,
            update_data: None,
            closed: false,
            last_updated: None,
        }
    }
}

impl crate::cxx_qt_lib_shoop::qquickitem::AsQQuickItem for BackendWrapper {
    unsafe fn mut_qquickitem_ptr(&mut self) -> *mut QQuickItem {
        qquickitem_from_ptr_backend_wrapper(self as *mut Self)
    }
    unsafe fn ref_qquickitem_ptr(&self) -> *const QQuickItem {
        qquickitem_from_ref_backend_wrapper(self) as *const QQuickItem
    }
}

impl crate::cxx_qt_lib_shoop::qquickitem::IsQQuickItem for BackendWrapper {}

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
