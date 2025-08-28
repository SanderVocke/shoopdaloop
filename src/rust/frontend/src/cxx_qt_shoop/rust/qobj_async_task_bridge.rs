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
        #[qproperty(bool, active, READ, NOTIFY=active_changed)]
        #[qproperty(bool, success, READ, WRITE, NOTIFY)]
        type AsyncTask = super::AsyncTaskRust;

        #[qinvokable]
        pub fn notify_done(self: Pin<&mut AsyncTask>, success: bool);

        #[qinvokable]
        pub fn finish_dummy(self: Pin<&mut AsyncTask>);

        // After execution is finished, execute the given callable,
        // then self-delete.
        // Meant to be called from QML (QJSValue).
        #[qinvokable]
        pub fn then(self: Pin<&mut AsyncTask>, func: QVariant);

        // After execution is finished, self-delete.
        #[qinvokable]
        pub fn then_delete(self: Pin<&mut AsyncTask>);

        // If then or then_delete is not called on the AsyncTask, it
        // will start a timer on completion and generate an error message.
        // one of these methods needs to be called for lifetime management.
        #[qinvokable]
        pub fn not_deleted_error(self: Pin<&mut AsyncTask>);

        #[qsignal]
        pub unsafe fn active_changed(self: Pin<&mut AsyncTask>, active: bool);
    }

    unsafe extern "RustQt" {
        #[qobject]
        type AsyncTaskNotifier = super::AsyncTaskNotifierRust;

        #[qsignal]
        pub unsafe fn notify_done(self: Pin<&mut AsyncTaskNotifier>, success: bool);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/register_qml_type.h");

        #[rust_name = "register_qml_type_async_task"]
        unsafe fn register_qml_type(
            inference_example: *mut AsyncTask,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );

        include!("cxx-qt-lib-shoop/qobject.h");

        #[rust_name = "from_qobject_ref_async_task"]
        unsafe fn fromQObjectRef(obj: &QObject, output: *mut *const AsyncTask);

        #[rust_name = "from_qobject_mut_async_task"]
        unsafe fn fromQObjectMut(obj: Pin<&mut QObject>, output: *mut *mut AsyncTask);

        #[rust_name = "async_task_qobject_from_ptr"]
        unsafe fn qobjectFromPtr(obj: *mut AsyncTask) -> *mut QObject;

        #[rust_name = "async_task_qobject_from_ref"]
        fn qobjectFromRef(obj: &AsyncTask) -> &QObject;

        include!("cxx-qt-lib-shoop/make_raw.h");
        #[rust_name = "make_raw_async_task_with_parent"]
        unsafe fn make_raw_with_one_arg(parent: *mut QObject) -> *mut AsyncTask;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qobject.h");

        #[rust_name = "from_qobject_ref_async_task_notifier"]
        unsafe fn fromQObjectRef(obj: &QObject, output: *mut *const AsyncTaskNotifier);

        #[rust_name = "from_qobject_mut_async_task_notifier"]
        unsafe fn fromQObjectMut(obj: Pin<&mut QObject>, output: *mut *mut AsyncTaskNotifier);

        #[rust_name = "async_task_notifier_qobject_from_ptr"]
        unsafe fn qobjectFromPtr(obj: *mut AsyncTaskNotifier) -> *mut QObject;

        #[rust_name = "async_task_notifier_qobject_from_ref"]
        fn qobjectFromRef(obj: &AsyncTaskNotifier) -> &QObject;

        include!("cxx-qt-lib-shoop/make_raw.h");
        #[rust_name = "make_raw_async_task_notifier"]
        unsafe fn make_raw() -> *mut AsyncTaskNotifier;

        include!("cxx-qt-lib-shoop/cast_ptr.h");
        #[rust_name = "async_task_notifier_to_qobject"]
        unsafe fn cast_ptr(obj: *mut AsyncTaskNotifier) -> *mut QObject;
    }

    impl cxx_qt::Constructor<(*mut QObject,)> for AsyncTask {}
}

use cxx_qt_lib_shoop::{qobject::AsQObject, qtimer::QTimer};
pub use ffi::AsyncTask;
use ffi::*;

#[derive(Default)]
pub struct AsyncTaskNotifierRust {}

pub struct AsyncTaskRust {
    pub active: bool,
    pub success: bool,
    pub timer: *mut QTimer,
    pub maybe_qml_callable: QVariant,
    pub delete_when_done: bool,
}

impl Default for AsyncTaskRust {
    fn default() -> Self {
        Self {
            active: true,
            success: false,
            timer: std::ptr::null_mut(),
            maybe_qml_callable: QVariant::default(),
            delete_when_done: false,
        }
    }
}

impl AsQObject for ffi::AsyncTask {
    unsafe fn mut_qobject_ptr(&mut self) -> *mut ffi::QObject {
        ffi::async_task_qobject_from_ptr(self as *mut Self)
    }

    unsafe fn ref_qobject_ptr(&self) -> *const ffi::QObject {
        ffi::async_task_qobject_from_ref(self) as *const ffi::QObject
    }
}

impl cxx_qt_lib_shoop::qobject::FromQObject for AsyncTask {
    unsafe fn ptr_from_qobject_ref(obj: &cxx_qt_lib_shoop::qobject::QObject) -> *const Self {
        let mut output: *const Self = std::ptr::null();
        from_qobject_ref_async_task(obj, &mut output as *mut *const Self);
        output
    }

    unsafe fn ptr_from_qobject_mut(
        obj: std::pin::Pin<&mut cxx_qt_lib_shoop::qobject::QObject>,
    ) -> *mut Self {
        let mut output: *mut Self = std::ptr::null_mut();
        from_qobject_mut_async_task(obj, &mut output as *mut *mut Self);
        output
    }
}

impl AsQObject for ffi::AsyncTaskNotifier {
    unsafe fn mut_qobject_ptr(&mut self) -> *mut ffi::QObject {
        ffi::async_task_notifier_qobject_from_ptr(self as *mut Self)
    }

    unsafe fn ref_qobject_ptr(&self) -> *const ffi::QObject {
        ffi::async_task_notifier_qobject_from_ref(self) as *const ffi::QObject
    }
}

impl cxx_qt_lib_shoop::qobject::FromQObject for AsyncTaskNotifier {
    unsafe fn ptr_from_qobject_ref(obj: &cxx_qt_lib_shoop::qobject::QObject) -> *const Self {
        let mut output: *const Self = std::ptr::null();
        from_qobject_ref_async_task_notifier(obj, &mut output as *mut *const Self);
        output
    }

    unsafe fn ptr_from_qobject_mut(
        obj: std::pin::Pin<&mut cxx_qt_lib_shoop::qobject::QObject>,
    ) -> *mut Self {
        let mut output: *mut Self = std::ptr::null_mut();
        from_qobject_mut_async_task_notifier(obj, &mut output as *mut *mut Self);
        output
    }
}

impl cxx_qt::Constructor<(*mut QObject,)> for AsyncTask {
    type BaseArguments = (*mut QObject,); // Will be passed to the base class constructor
    type InitializeArguments = (); // Will be passed to the "initialize" function
    type NewArguments = (); // Will be passed to the "new" function

    fn route_arguments(
        args: (*mut QObject,),
    ) -> (
        Self::NewArguments,
        Self::BaseArguments,
        Self::InitializeArguments,
    ) {
        ((), args, ())
    }

    fn new(_args: ()) -> AsyncTaskRust {
        AsyncTaskRust::default()
    }

    fn initialize(self: core::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        AsyncTask::initialize_impl(self);
    }
}
