use cxx::Exception;
use cxx_qt;
use cxx_qt_lib::{QList, QMap, QMapPair_QString_QVariant, QString, QVariant};

use crate::cxx_qt_lib_shoop::qobject::AsQObject;

pub const AUTO_CONNECTION : u32 = 0;
pub const DIRECT_CONNECTION : u32 = 1;
pub const QUEUED_CONNECTION : u32 = 2;
pub const BLOCKING_QUEUED_CONNECTION : u32 = 3;
pub const UNIQUE_CONNECTION : u32 = 0x80;
pub const SINGLE_SHOT_CONNECTION : u32 = 0x100;

#[cxx_qt::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qobject.h");
        type QObject = crate::cxx_qt_lib_shoop::qobject::QObject;

        include!("cxx-qt-lib/qlist.h");
        type QList_QVariant = cxx_qt_lib::QList<cxx_qt_lib::QVariant>;

        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qmap.h");
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;
    }

    // Below, add bridge definitions for all combinations of return values and arguments used
    // throughout the project.

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/invoke.h");

        #[rust_name = "invoke_noreturn_noargs"]
        unsafe fn invoke(obj: *mut QObject,
                         method : String,
                         connection_type : u32) -> Result<()>;

        #[rust_name = "invoke_i32_noargs"]
        unsafe fn invoke_with_return(obj: *mut QObject,
                                     method : String,
                                     connection_type : u32) -> Result<i32>;
        
        #[rust_name = "invoke_noreturn_i32"]
        unsafe fn invoke_one_arg(obj: *mut QObject,
                                 method : String,
                                 connection_type : u32,
                                 arg1 : i32) -> Result<()>;
        
        #[rust_name = "invoke_bool_qstring"]
        unsafe fn invoke_one_arg_with_return(obj: *mut QObject,
                                             method : String,
                                             connection_type : u32,
                                             arg1 : QString) -> Result<bool>;
            
        #[rust_name = "invoke_qmapqstringqvariant_noargs"]
        unsafe fn invoke_with_return(obj: *mut QObject,
                                            method : String,
                                            connection_type : u32) -> Result<QMap_QString_QVariant>;

        #[rust_name = "invoke_qlistqvariant_qstring_i32_i32"]
        unsafe fn invoke_three_args_with_return(obj: *mut QObject,
                                                method : String,
                                                connection_type : u32,
                                                arg1 : QString, arg2 : i32, arg3 : i32) -> Result<QList_QVariant>;
    }
}

pub trait Invokable<RetVal, Args> {
    fn invoke_fn_qobj(&mut self,
                      _method : String,
                      _connection_type : u32,
                      _args : &Args) -> Result<RetVal, Exception> {
        panic!("Invokable not implemented for return type {} and arguments type {}", std::any::type_name::<RetVal>(), std::any::type_name::<Args>());
    }
}

// Below, implement Invokable for all combinations of return values and arguments used throughout
// the project.

impl Invokable<(), ()> for ffi::QObject {
    fn invoke_fn_qobj(&mut self,
                      method : String,
                      connection_type : u32,
                      _args : &()) -> Result<(), Exception> {
        unsafe { ffi::invoke_noreturn_noargs(self as *mut ffi::QObject, method, connection_type) }
    }
}

impl Invokable<i32, ()> for ffi::QObject {
    fn invoke_fn_qobj(&mut self,
                      method : String,
                      connection_type : u32,
                      _args : &()) -> Result<i32, Exception> {
        unsafe { ffi::invoke_i32_noargs(self as *mut ffi::QObject, method, connection_type) }
    }
}

impl Invokable<(), i32> for ffi::QObject {
    fn invoke_fn_qobj(&mut self,
                      method : String,
                      connection_type : u32,
                      args : &i32) -> Result<(), Exception> {
        unsafe { ffi::invoke_noreturn_i32(self as *mut ffi::QObject, method, connection_type, *args) }
    }
}

impl Invokable<bool, QString> for ffi::QObject {
    fn invoke_fn_qobj(&mut self,
                      method : String,
                      connection_type : u32,
                      args : &ffi::QString) -> Result<bool, Exception> {
        unsafe { ffi::invoke_bool_qstring(self as *mut ffi::QObject, method, connection_type, args.clone()) }
    }
}

impl Invokable<QMap<QMapPair_QString_QVariant>, ()> for ffi::QObject {
    fn invoke_fn_qobj(&mut self,
                      method : String,
                      connection_type : u32,
                      _args : &()) -> Result<QMap<QMapPair_QString_QVariant>, Exception> {
        unsafe { ffi::invoke_qmapqstringqvariant_noargs(self as *mut ffi::QObject, method, connection_type) }
    }
}

impl Invokable<QList<QVariant>, (QString, i32, i32)> for ffi::QObject {
    fn invoke_fn_qobj(&mut self,
                      method : String,
                      connection_type : u32,
                      args : &(ffi::QString, i32, i32)) -> Result<QList<QVariant>, Exception> {
        unsafe { ffi::invoke_qlistqvariant_qstring_i32_i32(self as *mut ffi::QObject, method, connection_type, args.0.clone(), args.1, args.2) }
    }
}

pub trait Invoker<RetVal, Args> {
    fn invoke(&mut self, method: String, connection_type : u32, args: &Args) -> Result<RetVal, Exception>;
}

impl<RetVal, Args> Invoker<RetVal, Args> for ffi::QObject
where
    Self: Invokable<RetVal, Args>,
{
    fn invoke(&mut self, method: String, connection_type : u32, args: &Args) -> Result<RetVal, Exception> {
        self.invoke_fn_qobj(method, connection_type, args)
    }
}

fn invoke_fn_qobj<RetVal, Args>(
    obj: &mut ffi::QObject,
    method: String,
    connection_type : u32,
    args: &Args,
) -> Result<RetVal, Exception>
where
    ffi::QObject: Invoker<RetVal, Args>,
{
    obj.invoke(method, connection_type, args)
}

pub trait QObjectOrConvertible {
    fn qobject_mut(&mut self) -> *mut ffi::QObject;
}

impl<T> QObjectOrConvertible for T
where
    T: AsQObject,
{
    fn qobject_mut(&mut self) -> *mut ffi::QObject {
        unsafe{ self.qobject_mut() }
    }
}

impl QObjectOrConvertible for crate::cxx_qt_lib_shoop::qobject::QObject {
    fn qobject_mut(&mut self) -> *mut ffi::QObject {
        self as *mut ffi::QObject
    }
}

// Generically invoke a method using Qt's meta-object system. The Object should implement
// AsQObject in order to be converted to its base class QObject.
// Can only be used with combinations for RetVal and Args that have been implemented
// above - if missing one can always be added.
pub fn invoke<Object, RetVal, Args>(
    obj: &mut Object,
    method: String,
    connection_type : u32,
    args: &Args,
) -> Result<RetVal, Exception>
where
    Object : QObjectOrConvertible,
    ffi::QObject: Invoker<RetVal, Args>,
{
    unsafe {
        let qobj = obj.qobject_mut().as_mut().unwrap();
        invoke_fn_qobj(qobj, method, connection_type, args)
    }
}