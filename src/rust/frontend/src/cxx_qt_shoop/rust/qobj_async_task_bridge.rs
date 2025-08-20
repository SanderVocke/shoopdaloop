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
        type AsyncTask = super::AsyncTaskRust;

        #[qinvokable]
        pub fn notify_done(self: Pin<&mut AsyncTask>);

        #[qinvokable]
        pub fn then(self: Pin<&mut AsyncTask>, func: QVariant);

        #[qsignal]
        pub unsafe fn active_changed(self: Pin<&mut AsyncTask>, active: bool);
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
    }
}

use cxx_qt_lib_shoop::qobject::AsQObject;
pub use ffi::AsyncTask;
use ffi::*;

pub struct AsyncTaskRust {
    pub active: bool,
}

impl Default for AsyncTaskRust {
    fn default() -> Self {
        Self {
            active: false,
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
