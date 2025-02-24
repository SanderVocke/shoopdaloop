use cxx::Exception;
use cxx_qt;
use cxx_qt_lib::{QList, QVariant, QString};

use crate::cxx_qt_lib_shoop::qobject::AsQObject;

#[cxx_qt::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qobject.h");
        type QObject = crate::cxx_qt_lib_shoop::qobject::QObject;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qlist.h");
        type QList_QVariant = cxx_qt_lib::QList<cxx_qt_lib::QVariant>;

        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    // Below, add bridge definitions for all combinations of return values and arguments used
    // throughout the project.

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/invoke.h");

        #[rust_name = "invoke_noreturn_noargs"]
        unsafe fn invoke(obj: *mut QObject, method : String) -> Result<()>;

        #[rust_name = "invoke_i32_noargs"]
        unsafe fn invoke_with_return(obj: *mut QObject, method : String) -> Result<i32>;

        #[rust_name = "invoke_qlistqvariant_qstring_i32_i32"]
        unsafe fn invoke_three_args_with_return(obj: *mut QObject, method : String, arg1 : QString, arg2 : i32, arg3 : i32) -> Result<QList_QVariant>;
    }
}

trait Invokable<RetVal, Args> {
    fn invoke_fn_qobj(&mut self, method : String, args : &Args) -> Result<RetVal, Exception> {
        panic!("Invokable not implemented for return type {} and arguments type {}", std::any::type_name::<RetVal>(), std::any::type_name::<Args>());
    }
}

// Below, implement Invokable for all combinations of return values and arguments used throughout
// the project.

impl Invokable<(), ()> for ffi::QObject {
    fn invoke_fn_qobj(&mut self, method : String, _args : &()) -> Result<(), Exception> {
        unsafe { ffi::invoke_noreturn_noargs(self as *mut ffi::QObject, method) }
    }
}

impl Invokable<i32, ()> for ffi::QObject {
    fn invoke_fn_qobj(&mut self, method : String, _args : &()) -> Result<i32, Exception> {
        unsafe { ffi::invoke_i32_noargs(self as *mut ffi::QObject, method) }
    }
}

impl Invokable<QList<QVariant>, (QString, i32, i32)> for ffi::QObject {
    fn invoke_fn_qobj(&mut self, method : String, args : &(ffi::QString, i32, i32)) -> Result<QList<QVariant>, Exception> {
        unsafe { ffi::invoke_qlistqvariant_qstring_i32_i32(self as *mut ffi::QObject, method, args.0.clone(), args.1, args.2) }
    }
}

trait Invoker<RetVal, Args> {
    fn invoke(&mut self, method: String, args: &Args) -> Result<RetVal, Exception>;
}

impl<RetVal, Args> Invoker<RetVal, Args> for ffi::QObject
where
    Self: Invokable<RetVal, Args>,
{
    fn invoke(&mut self, method: String, args: &Args) -> Result<RetVal, Exception> {
        self.invoke_fn_qobj(method, args)
    }
}

fn invoke_fn_qobj<RetVal, Args>(
    obj: &mut ffi::QObject,
    method: String,
    args: &Args,
) -> Result<RetVal, Exception>
where
    ffi::QObject: Invoker<RetVal, Args>,
{
    obj.invoke(method, args)
}

// Generically invoke a method using Qt's meta-object system. The Object should implement
// AsQObject in order to be converted to its base class QObject.
// Can only be used with combinations for RetVal and Args that have been implemented
// above - if missing one can always be added.
pub fn invoke<Object, RetVal, Args>(
    obj: &mut Object,
    method: String,
    args: &Args,
) -> Result<RetVal, Exception>
where
    Object : AsQObject,
    ffi::QObject: Invoker<RetVal, Args>,
{
    unsafe {
        let qobj = obj.qobject_mut();
        invoke_fn_qobj(qobj, method, args)
    }
}