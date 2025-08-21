use cxx::Exception;
use cxx_qt;
use cxx_qt_lib::{QList, QMap, QMapPair_QString_QVariant, QString, QVariant};

use crate::qobject::AsQObject;

pub const AUTO_CONNECTION: u32 = 0;
pub const DIRECT_CONNECTION: u32 = 1;
pub const QUEUED_CONNECTION: u32 = 2;
pub const BLOCKING_QUEUED_CONNECTION: u32 = 3;
pub const UNIQUE_CONNECTION: u32 = 0x80;
pub const SINGLE_SHOT_CONNECTION: u32 = 0x100;

#[cxx_qt::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qobject.h");
        type QObject = crate::qobject::QObject;

        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qmap.h");
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;

        include!("cxx-qt-lib/qlist.h");
        type QList_QVariant = cxx_qt_lib::QList<cxx_qt_lib::QVariant>;
        type QList_QString = cxx_qt_lib::QList<cxx_qt_lib::QString>;
        type QList_f32 = cxx_qt_lib::QList<f32>;
    }

    // Below, add bridge definitions for all combinations of return values and arguments used
    // throughout the project.

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/invoke.h");

        #[rust_name = "invoke_noreturn_noargs"]
        unsafe fn invoke(obj: *mut QObject, method: String, connection_type: u32) -> Result<()>;

        #[rust_name = "invoke_i32_noargs"]
        unsafe fn invoke_with_return(
            obj: *mut QObject,
            method: String,
            connection_type: u32,
        ) -> Result<i32>;

        #[rust_name = "invoke_noreturn_i32"]
        unsafe fn invoke_one_arg(
            obj: *mut QObject,
            method: String,
            connection_type: u32,
            arg1: i32,
        ) -> Result<()>;

        #[rust_name = "invoke_noreturn_i32_i32_i32"]
        unsafe fn invoke_three_args(
            obj: *mut QObject,
            method: String,
            connection_type: u32,
            arg1: i32,
            arg2: i32,
            arg3: i32,
        ) -> Result<()>;

        #[rust_name = "invoke_noreturn_qlistqvariant_i32_i32_i32"]
        unsafe fn invoke_four_args(
            obj: *mut QObject,
            method: String,
            connection_type: u32,
            arg1: QList_QVariant,
            arg2: i32,
            arg3: i32,
            arg4: i32,
        ) -> Result<()>;

        #[rust_name = "invoke_noreturn_qvariant_qvariant_qvariant_i32"]
        unsafe fn invoke_four_args(
            obj: *mut QObject,
            method: String,
            connection_type: u32,
            arg1: QVariant,
            arg2: QVariant,
            arg3: QVariant,
            arg4: i32,
        ) -> Result<()>;

        #[rust_name = "invoke_bool_qstring"]
        unsafe fn invoke_one_arg_with_return(
            obj: *mut QObject,
            method: String,
            connection_type: u32,
            arg1: QString,
        ) -> Result<bool>;

        #[rust_name = "invoke_bool_noargs"]
        unsafe fn invoke_with_return(
            obj: *mut QObject,
            method: String,
            connection_type: u32,
        ) -> Result<bool>;

        #[rust_name = "invoke_qvariant_noargs"]
        unsafe fn invoke_with_return(
            obj: *mut QObject,
            method: String,
            connection_type: u32,
        ) -> Result<QVariant>;

        #[rust_name = "invoke_qmapqstringqvariant_noargs"]
        unsafe fn invoke_with_return(
            obj: *mut QObject,
            method: String,
            connection_type: u32,
        ) -> Result<QMap_QString_QVariant>;

        #[rust_name = "invoke_noreturn_qlistf32"]
        unsafe fn invoke_one_arg(
            obj: *mut QObject,
            method: String,
            connection_type: u32,
            arg1: QList_f32,
        ) -> Result<()>;

        #[rust_name = "invoke_noreturn_bool"]
        unsafe fn invoke_one_arg(
            obj: *mut QObject,
            method: String,
            connection_type: u32,
            arg1: bool,
        ) -> Result<()>;

        #[rust_name = "invoke_qlistqvariant_noargs"]
        unsafe fn invoke_with_return(
            obj: *mut QObject,
            method: String,
            connection_type: u32,
        ) -> Result<QList_QVariant>;

        #[rust_name = "invoke_noreturn_qlistqvariant"]
        unsafe fn invoke_one_arg(
            obj: *mut QObject,
            method: String,
            connection_type: u32,
            arg1: QList_QVariant,
        ) -> Result<()>;

        #[rust_name = "invoke_qlistqvariant_qstring_i32_i32"]
        unsafe fn invoke_three_args_with_return(
            obj: *mut QObject,
            method: String,
            connection_type: u32,
            arg1: QString,
            arg2: i32,
            arg3: i32,
        ) -> Result<QList_QVariant>;

        #[rust_name = "invoke_qlistqstring_noargs"]
        unsafe fn invoke_with_return(
            obj: *mut QObject,
            method: String,
            connection_type: u32,
        ) -> Result<QList_QString>;

        #[rust_name = "invoke_qlistf32_noargs"]
        unsafe fn invoke_with_return(
            obj: *mut QObject,
            method: String,
            connection_type: u32,
        ) -> Result<QList_f32>;

        #[rust_name = "invoke_qlistf32_i32"]
        unsafe fn invoke_one_arg_with_return(
            obj: *mut QObject,
            method: String,
            connection_type: u32,
            arg1: i32,
        ) -> Result<QList_f32>;

        #[rust_name = "invoke_qobject_noargs"]
        unsafe fn invoke_with_return(
            obj: *mut QObject,
            method: String,
            connection_type: u32,
        ) -> Result<*mut QObject>;

        #[rust_name = "invoke_qstring_noargs"]
        unsafe fn invoke_with_return(
            obj: *mut QObject,
            method: String,
            connection_type: u32,
        ) -> Result<QString>;
    }
}

pub trait Invokable<RetVal, Args> {
    fn invoke_fn_qobj(
        &mut self,
        _method: &str,
        _connection_type: u32,
        _args: &Args,
    ) -> Result<RetVal, Exception> {
        panic!(
            "Invokable not implemented for return type {} and arguments type {}",
            std::any::type_name::<RetVal>(),
            std::any::type_name::<Args>()
        );
    }
}

// Below, implement Invokable for all combinations of return values and arguments used throughout
// the project.

impl Invokable<(), ()> for ffi::QObject {
    fn invoke_fn_qobj(
        &mut self,
        method: &str,
        connection_type: u32,
        _args: &(),
    ) -> Result<(), Exception> {
        unsafe {
            ffi::invoke_noreturn_noargs(
                self as *mut ffi::QObject,
                method.to_string(),
                connection_type,
            )
        }
    }
}

impl Invokable<i32, ()> for ffi::QObject {
    fn invoke_fn_qobj(
        &mut self,
        method: &str,
        connection_type: u32,
        _args: &(),
    ) -> Result<i32, Exception> {
        unsafe {
            ffi::invoke_i32_noargs(
                self as *mut ffi::QObject,
                method.to_string(),
                connection_type,
            )
        }
    }
}

impl Invokable<bool, ()> for ffi::QObject {
    fn invoke_fn_qobj(
        &mut self,
        method: &str,
        connection_type: u32,
        _args: &(),
    ) -> Result<bool, Exception> {
        unsafe {
            ffi::invoke_bool_noargs(
                self as *mut ffi::QObject,
                method.to_string(),
                connection_type,
            )
        }
    }
}

impl Invokable<QVariant, ()> for ffi::QObject {
    fn invoke_fn_qobj(
        &mut self,
        method: &str,
        connection_type: u32,
        _args: &(),
    ) -> Result<QVariant, Exception> {
        unsafe {
            ffi::invoke_qvariant_noargs(
                self as *mut ffi::QObject,
                method.to_string(),
                connection_type,
            )
        }
    }
}

impl Invokable<(), i32> for ffi::QObject {
    fn invoke_fn_qobj(
        &mut self,
        method: &str,
        connection_type: u32,
        args: &i32,
    ) -> Result<(), Exception> {
        unsafe {
            ffi::invoke_noreturn_i32(
                self as *mut ffi::QObject,
                method.to_string(),
                connection_type,
                *args,
            )
        }
    }
}

impl Invokable<(), (i32, i32, i32)> for ffi::QObject {
    fn invoke_fn_qobj(
        &mut self,
        method: &str,
        connection_type: u32,
        args: &(i32, i32, i32),
    ) -> Result<(), Exception> {
        unsafe {
            ffi::invoke_noreturn_i32_i32_i32(
                self as *mut ffi::QObject,
                method.to_string(),
                connection_type,
                args.0,
                args.1,
                args.2,
            )
        }
    }
}

impl Invokable<(), ffi::QList_f32> for ffi::QObject {
    fn invoke_fn_qobj(
        &mut self,
        method: &str,
        connection_type: u32,
        arg: &ffi::QList_f32,
    ) -> Result<(), Exception> {
        unsafe {
            ffi::invoke_noreturn_qlistf32(
                self as *mut ffi::QObject,
                method.to_string(),
                connection_type,
                arg.clone(),
            )
        }
    }
}

impl Invokable<(), bool> for ffi::QObject {
    fn invoke_fn_qobj(
        &mut self,
        method: &str,
        connection_type: u32,
        arg: &bool,
    ) -> Result<(), Exception> {
        unsafe {
            ffi::invoke_noreturn_bool(
                self as *mut ffi::QObject,
                method.to_string(),
                connection_type,
                *arg,
            )
        }
    }
}

impl Invokable<(), (ffi::QList_QVariant, i32, i32, i32)> for ffi::QObject {
    fn invoke_fn_qobj(
        &mut self,
        method: &str,
        connection_type: u32,
        args: &(ffi::QList_QVariant, i32, i32, i32),
    ) -> Result<(), Exception> {
        unsafe {
            ffi::invoke_noreturn_qlistqvariant_i32_i32_i32(
                self as *mut ffi::QObject,
                method.to_string(),
                connection_type,
                args.0.clone(),
                args.1,
                args.2,
                args.3,
            )
        }
    }
}

impl Invokable<(), (QVariant, QVariant, QVariant, i32)> for ffi::QObject {
    fn invoke_fn_qobj(
        &mut self,
        method: &str,
        connection_type: u32,
        args: &(QVariant, QVariant, QVariant, i32),
    ) -> Result<(), Exception> {
        unsafe {
            ffi::invoke_noreturn_qvariant_qvariant_qvariant_i32(
                self as *mut ffi::QObject,
                method.to_string(),
                connection_type,
                args.0.clone(),
                args.1.clone(),
                args.2.clone(),
                args.3,
            )
        }
    }
}

impl Invokable<bool, QString> for ffi::QObject {
    fn invoke_fn_qobj(
        &mut self,
        method: &str,
        connection_type: u32,
        args: &ffi::QString,
    ) -> Result<bool, Exception> {
        unsafe {
            ffi::invoke_bool_qstring(
                self as *mut ffi::QObject,
                method.to_string(),
                connection_type,
                args.clone(),
            )
        }
    }
}

impl Invokable<QMap<QMapPair_QString_QVariant>, ()> for ffi::QObject {
    fn invoke_fn_qobj(
        &mut self,
        method: &str,
        connection_type: u32,
        _args: &(),
    ) -> Result<QMap<QMapPair_QString_QVariant>, Exception> {
        unsafe {
            ffi::invoke_qmapqstringqvariant_noargs(
                self as *mut ffi::QObject,
                method.to_string(),
                connection_type,
            )
        }
    }
}

impl Invokable<QList<QVariant>, ()> for ffi::QObject {
    fn invoke_fn_qobj(
        &mut self,
        method: &str,
        connection_type: u32,
        _args: &(),
    ) -> Result<QList<QVariant>, Exception> {
        unsafe {
            ffi::invoke_qlistqvariant_noargs(
                self as *mut ffi::QObject,
                method.to_string(),
                connection_type,
            )
        }
    }
}

impl Invokable<(), ffi::QList_QVariant> for ffi::QObject {
    fn invoke_fn_qobj(
        &mut self,
        method: &str,
        connection_type: u32,
        arg: &ffi::QList_QVariant,
    ) -> Result<(), Exception> {
        unsafe {
            ffi::invoke_noreturn_qlistqvariant(
                self as *mut ffi::QObject,
                method.to_string(),
                connection_type,
                arg.clone(),
            )
        }
    }
}

impl Invokable<QList<QVariant>, (QString, i32, i32)> for ffi::QObject {
    fn invoke_fn_qobj(
        &mut self,
        method: &str,
        connection_type: u32,
        args: &(ffi::QString, i32, i32),
    ) -> Result<QList<QVariant>, Exception> {
        unsafe {
            ffi::invoke_qlistqvariant_qstring_i32_i32(
                self as *mut ffi::QObject,
                method.to_string(),
                connection_type,
                args.0.clone(),
                args.1,
                args.2,
            )
        }
    }
}

impl Invokable<ffi::QList_QString, ()> for ffi::QObject {
    fn invoke_fn_qobj(
        &mut self,
        method: &str,
        connection_type: u32,
        _args: &(),
    ) -> Result<ffi::QList_QString, Exception> {
        unsafe {
            ffi::invoke_qlistqstring_noargs(
                self as *mut ffi::QObject,
                method.to_string(),
                connection_type,
            )
        }
    }
}

impl Invokable<ffi::QList_f32, ()> for ffi::QObject {
    fn invoke_fn_qobj(
        &mut self,
        method: &str,
        connection_type: u32,
        _args: &(),
    ) -> Result<ffi::QList_f32, Exception> {
        unsafe {
            ffi::invoke_qlistf32_noargs(
                self as *mut ffi::QObject,
                method.to_string(),
                connection_type,
            )
        }
    }
}

impl Invokable<ffi::QList_f32, i32> for ffi::QObject {
    fn invoke_fn_qobj(
        &mut self,
        method: &str,
        connection_type: u32,
        arg: &i32,
    ) -> Result<ffi::QList_f32, Exception> {
        unsafe {
            ffi::invoke_qlistf32_i32(
                self as *mut ffi::QObject,
                method.to_string(),
                connection_type,
                *arg,
            )
        }
    }
}

impl Invokable<*mut ffi::QObject, ()> for ffi::QObject {
    fn invoke_fn_qobj(
        &mut self,
        method: &str,
        connection_type: u32,
        _args: &(),
    ) -> Result<*mut ffi::QObject, Exception> {
        unsafe {
            ffi::invoke_qobject_noargs(
                self as *mut ffi::QObject,
                method.to_string(),
                connection_type,
            )
        }
    }
}

impl Invokable<ffi::QString, ()> for ffi::QObject {
    fn invoke_fn_qobj(
        &mut self,
        method: &str,
        connection_type: u32,
        _args: &(),
    ) -> Result<ffi::QString, Exception> {
        unsafe {
            ffi::invoke_qstring_noargs(
                self as *mut ffi::QObject,
                method.to_string(),
                connection_type,
            )
        }
    }
}

pub trait Invoker<RetVal, Args> {
    fn invoke(
        &mut self,
        method: &str,
        connection_type: u32,
        args: &Args,
    ) -> Result<RetVal, Exception>;
}

impl<RetVal, Args> Invoker<RetVal, Args> for ffi::QObject
where
    Self: Invokable<RetVal, Args>,
{
    fn invoke(
        &mut self,
        method: &str,
        connection_type: u32,
        args: &Args,
    ) -> Result<RetVal, Exception> {
        self.invoke_fn_qobj(method, connection_type, args)
    }
}

fn invoke_fn_qobj<RetVal, Args>(
    obj: &mut ffi::QObject,
    method: &str,
    connection_type: u32,
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
        unsafe { self.qobject_mut() }
    }
}

impl QObjectOrConvertible for crate::qobject::QObject {
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
    method: &str,
    connection_type: u32,
    args: &Args,
) -> Result<RetVal, Exception>
where
    Object: QObjectOrConvertible,
    ffi::QObject: Invoker<RetVal, Args>,
{
    unsafe {
        let qobj = obj.qobject_mut().as_mut().unwrap();
        invoke_fn_qobj(qobj, method, connection_type, args)
    }
}
