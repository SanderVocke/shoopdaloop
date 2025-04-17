use cxx::Exception;
use crate::cxx_qt_lib_shoop::qobject::AsQObject;

#[cxx_qt::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qobject.h");
        type QObject = crate::cxx_qt_lib_shoop::qobject::QObject;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/connect.h");

        #[rust_name = "connect"]
        unsafe fn connect(obj: *const QObject,
                          signal : String,
                          receiver : *const QObject,
                          slot : String,
                          connection_type : u32) -> Result<()>;
    }
}



pub trait QObjectOrConvertible {
    fn qobject_mut(&mut self) -> *mut ffi::QObject;
    fn qobject_ref(&self) -> *const ffi::QObject;
}

impl<T> QObjectOrConvertible for T
where
    T: AsQObject,
{
    fn qobject_mut(&mut self) -> *mut ffi::QObject {
        unsafe{ self.qobject_mut() }
    }

    fn qobject_ref(&self) -> *const ffi::QObject {
        unsafe{ self.qobject_ref() }
    }
}

impl QObjectOrConvertible for crate::cxx_qt_lib_shoop::qobject::QObject {
    fn qobject_mut(&mut self) -> *mut ffi::QObject {
        self as *mut ffi::QObject
    }
    fn qobject_ref(&self) -> *const ffi::QObject {
        self as *const ffi::QObject
    }
}

// Generically connect methods of cxx_qt objects or QObject pointers
pub fn connect<Sender, Receiver>(
    sender : &Sender,
    signal : String,
    receiver : &Receiver,
    slot : String,
    connection_type : u32,
) -> Result<(), Exception>
where
    Sender : QObjectOrConvertible,
    Receiver : QObjectOrConvertible,
{
    unsafe {
        let sender_qobj = sender.qobject_ref();
        let receiver_qobj = receiver.qobject_ref();
        ffi::connect(sender_qobj, signal, receiver_qobj, slot, connection_type)
    }
}