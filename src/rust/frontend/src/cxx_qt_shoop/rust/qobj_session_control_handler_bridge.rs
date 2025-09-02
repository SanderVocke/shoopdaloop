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

        include!("cxx-qt-lib/qlist.h");
        type QList_QVariant = cxx_qt_lib::QList<cxx_qt_lib::QVariant>;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qproperty(QList_QVariant, loop_references, READ=get_loop_references, WRITE=set_loop_references)]
        type SessionControlHandler = super::SessionControlHandlerRust;

        #[qinvokable]
        pub fn install_on_lua_engine(self: Pin<&mut SessionControlHandler>, engine: *mut QObject);

        #[qinvokable]
        pub fn set_loop_references(
            self: Pin<&mut SessionControlHandler>,
            loop_references: QList_QVariant,
        );

        #[qinvokable]
        pub fn get_loop_references(self: &SessionControlHandler) -> QList_QVariant;
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

use std::{cell::RefCell, collections::{BTreeMap, HashSet}, rc::{Rc, Weak}};

use cxx_qt_lib_shoop::qpointer::QPointerQObject;
use ffi::*;

use crate::{lua_callback::LuaCallback, lua_engine::LuaEngineImpl};

pub struct SessionControlHandlerLuaTarget {
    pub loop_references: BTreeMap<(i64, i64), cxx::UniquePtr<QPointerQObject>>,
    pub weak_self: Weak<RefCell<SessionControlHandlerLuaTarget>>,
    pub callbacks: Vec<Rc<Box<dyn LuaCallback>>>,
}

pub struct SessionControlHandlerRust {
    pub lua_target: Rc<RefCell<SessionControlHandlerLuaTarget>>,
}

impl Default for SessionControlHandlerRust {
    fn default() -> Self {
        let target = Rc::new(RefCell::new(SessionControlHandlerLuaTarget {
                loop_references: BTreeMap::new(),
                weak_self: std::rc::Weak::new(),
                callbacks: Vec::default(),
            }));
        let weak = Rc::downgrade(&target);
        target.borrow_mut().weak_self = weak;

        Self {
            lua_target: target
        }
    }
}
