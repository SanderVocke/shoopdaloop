#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib-shoop/qobject.h");
        type QObject = cxx_qt_lib_shoop::qobject::QObject;

        include!("cxx-qt-lib/qmap.h");
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;
    }

    unsafe extern "RustQt" {
        #[qobject]
        type SessionControlHandler = super::SessionControlHandlerRust;

        #[qinvokable]
        pub fn install_on_lua_engine(self: Pin<&mut SessionControlHandler>, engine: *mut QObject);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/register_qml_type.h");

        #[rust_name = "register_qml_type_session_control_handler"]
        unsafe fn register_qml_type(
            inference_example: *mut SessionControlHandler,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );
    }
}

use std::{cell::RefCell, collections::BTreeMap, rc::{Rc, Weak}};

use cxx_qt_lib_shoop::qpointer::QPointerQObject;
use ffi::*;

pub struct SessionControlHandlerLuaTarget {
    pub loop_references: BTreeMap<(i64, i64), cxx::UniquePtr<QPointerQObject>>,
    pub weak_self: Weak<RefCell<SessionControlHandlerLuaTarget>>,
}

pub struct SessionControlHandlerRust {
    pub lua_target: Rc<RefCell<SessionControlHandlerLuaTarget>>,
}

impl Default for SessionControlHandlerRust {
    fn default() -> Self {
        let target = Rc::new(RefCell::new(SessionControlHandlerLuaTarget {
                loop_references: BTreeMap::new(),
                weak_self: std::rc::Weak::new(),
            }));
        let weak = Rc::downgrade(&target);
        target.borrow_mut().weak_self = weak;

        Self {
            lua_target: target
        }
    }
}
