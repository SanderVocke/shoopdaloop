#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");
        type QQuickItem = cxx_qt_lib_shoop::qquickitem::QQuickItem;

        include!("cxx-qt-lib-shoop/qobject.h");
        type QObject = cxx_qt_lib_shoop::qobject::QObject;

        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qmap.h");
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qproperty(QString, name)]
        #[qproperty(QString, instance_identifier)]
        type Logger = super::LoggerRust;

        #[qinvokable]
        pub fn trace(self: &Logger, msg: QString);

        #[qinvokable]
        pub fn debug(self: &Logger, msg: QString);

        #[qinvokable]
        pub fn info(self: &Logger, msg: QString);

        #[qinvokable]
        pub fn warning(self: &Logger, msg: QString);

        #[qinvokable]
        pub fn error(self: &Logger, msg: QString);

        #[qinvokable]
        pub fn should_trace(self: &Logger) -> bool;

        #[qinvokable]
        pub fn should_debug(self: &Logger) -> bool;

        #[qinvokable]
        pub fn create_logger(self: Pin<&mut Logger>, name: QString);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/register_qml_type.h");
        #[rust_name = "register_qml_type_logger"]
        unsafe fn register_qml_type(
            inference_example: *mut Logger,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );
    }

    impl cxx_qt::Constructor<(*mut QObject,), NewArguments = (*mut QObject,)> for Logger {}
    impl cxx_qt::Constructor<(), NewArguments = ()> for Logger {}
}
use backend_bindings::Logger as BackendLogger;
use ffi::*;

#[derive(Default)]
pub struct LoggerRust {
    pub name: QString,
    pub instance_identifier: QString,
    pub backend: Option<BackendLogger>,
}

impl cxx_qt::Constructor<(*mut QObject,)> for Logger {
    type BaseArguments = (*mut QObject,); // Will be passed to the base class constructor
    type InitializeArguments = (); // Will be passed to the "initialize" function
    type NewArguments = (*mut QObject,); // Will be passed to the "new" function

    fn route_arguments(
        args: (*mut QObject,),
    ) -> (
        Self::NewArguments,
        Self::BaseArguments,
        Self::InitializeArguments,
    ) {
        (args, args, ())
    }

    fn new(_parent: (*mut QObject,)) -> LoggerRust {
        LoggerRust::default()
    }

    fn initialize(self: std::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        Logger::initialize_impl(self);
    }
}

impl cxx_qt::Constructor<()> for Logger {
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

    fn new(_args: ()) -> LoggerRust {
        LoggerRust::default()
    }

    fn initialize(self: std::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        Logger::initialize_impl(self);
    }
}
