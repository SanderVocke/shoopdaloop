#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib-shoop/qobject.h");
        type QObject = cxx_qt_lib_shoop::qobject::QObject;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qproperty(bool, active, READ=get_active, NOTIFY=active_changed)]
        type AsyncTasks = super::AsyncTasksRust;

        #[qinvokable]
        pub fn notify_task_active_changed(self: Pin<&mut AsyncTasks>, task_active: bool);

        #[qinvokable]
        pub fn then(self: Pin<&mut AsyncTasks>, func: QVariant);

        #[qinvokable]
        pub fn get_active(self: &AsyncTasks) -> bool;

        #[qinvokable]
        pub fn add_subtask(self: Pin<&mut AsyncTasks>, subtask: *mut QObject);

        #[qsignal]
        pub unsafe fn active_changed(self: Pin<&mut AsyncTasks>, active: bool);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/register_qml_type.h");

        #[rust_name = "register_qml_type_async_tasks"]
        unsafe fn register_qml_type(
            inference_example: *mut AsyncTasks,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );

        include!("cxx-qt-lib-shoop/qobject.h");

        #[rust_name = "async_tasks_qobject_from_ptr"]
        unsafe fn qobjectFromPtr(obj: *mut AsyncTasks) -> *mut QObject;

        #[rust_name = "async_tasks_qobject_from_ref"]
        fn qobjectFromRef(obj: &AsyncTasks) -> &QObject;
    }

    impl cxx_qt::Constructor<()> for AsyncTasks {}
}

use cxx_qt_lib::QVariant;
use cxx_qt_lib_shoop::{qobject::AsQObject, qtimer::QTimer};
pub use ffi::AsyncTasks;

pub struct AsyncTasksRust {
    pub n_tracking: u32,
    pub n_done: u32,
    pub timer: *mut QTimer,
    pub maybe_qml_callable: QVariant,
    pub delete_when_done: bool,
}

impl Default for AsyncTasksRust {
    fn default() -> Self {
        Self {
            n_tracking: 0,
            n_done: 0,
            timer: std::ptr::null_mut(),
            maybe_qml_callable: QVariant::default(),
            delete_when_done: false,
        }
    }
}

impl AsQObject for ffi::AsyncTasks {
    unsafe fn mut_qobject_ptr(&mut self) -> *mut ffi::QObject {
        ffi::async_tasks_qobject_from_ptr(self as *mut Self)
    }

    unsafe fn ref_qobject_ptr(&self) -> *const ffi::QObject {
        ffi::async_tasks_qobject_from_ref(self) as *const ffi::QObject
    }
}

impl cxx_qt::Constructor<()> for AsyncTasks {
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

    fn new(_args: ()) -> AsyncTasksRust {
        AsyncTasksRust::default()
    }

    fn initialize(self: core::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        AsyncTasks::initialize_impl(self);
    }
}
