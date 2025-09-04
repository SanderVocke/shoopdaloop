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
        #[qproperty(QList_QVariant, loop_widget_references)]
        #[qproperty(QList_QVariant, track_control_widget_references)]
        #[qproperty(QList_QVariant, selected_loops, READ=get_selected_loops, WRITE=set_selected_loops, NOTIFY=selected_loops_changed)]
        #[qproperty(QVariant, targeted_loop, READ=get_targeted_loop, WRITE=set_targeted_loop, NOTIFY=targeted_loop_changed)]
        #[qproperty(*mut QObject, global_state_registry, READ=get_global_state_registry, WRITE=set_global_state_registry, NOTIFY=global_state_registry_changed)]
        #[qproperty(*mut QObject, session, READ=get_session, WRITE=set_session, NOTIFY=session_changed)]
        type SessionControlHandler = super::SessionControlHandlerRust;

        #[qinvokable]
        pub fn install_on_lua_engine(self: Pin<&mut SessionControlHandler>, engine: *mut QObject);

        #[qinvokable]
        pub fn update_structured_loop_widget_references(self: Pin<&mut SessionControlHandler>);

        #[qinvokable]
        pub fn update_structured_track_control_widget_references(
            self: Pin<&mut SessionControlHandler>,
        );

        #[qinvokable]
        pub fn set_selected_loops(self: Pin<&mut SessionControlHandler>, loops: QList_QVariant);

        #[qinvokable]
        pub fn set_targeted_loop(self: Pin<&mut SessionControlHandler>, maybe_loop: QVariant);

        #[qinvokable]
        pub fn get_selected_loops(self: Pin<&mut SessionControlHandler>) -> QList_QVariant;

        #[qinvokable]
        pub fn get_targeted_loop(self: Pin<&mut SessionControlHandler>) -> QVariant;

        #[qinvokable]
        pub fn set_session(self: Pin<&mut SessionControlHandler>, session: *mut QObject);

        #[qinvokable]
        pub fn get_session(self: Pin<&mut SessionControlHandler>) -> *mut QObject;

        #[qinvokable]
        pub fn set_global_state_registry(
            self: Pin<&mut SessionControlHandler>,
            registry: *mut QObject,
        );

        #[qinvokable]
        pub fn get_global_state_registry(self: Pin<&mut SessionControlHandler>) -> *mut QObject;

        #[qsignal]
        pub fn session_changed(self: Pin<&mut SessionControlHandler>);

        #[qsignal]
        pub fn selected_loops_changed(self: Pin<&mut SessionControlHandler>);

        #[qsignal]
        pub fn targeted_loop_changed(self: Pin<&mut SessionControlHandler>);

        #[qsignal]
        pub fn global_state_registry_changed(self: Pin<&mut SessionControlHandler>);
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

        include!("cxx-qt-lib-shoop/qobject.h");

        #[rust_name = "session_control_handler_qobject_from_ptr"]
        unsafe fn qobjectFromPtr(obj: *mut SessionControlHandler) -> *mut QObject;

        #[rust_name = "session_control_handler_qobject_from_ref"]
        fn qobjectFromRef(obj: &SessionControlHandler) -> &QObject;
    }

    impl cxx_qt::Constructor<(*mut QObject,), NewArguments = (*mut QObject,)>
        for SessionControlHandler
    {
    }
    impl cxx_qt::Constructor<(), NewArguments = ()> for SessionControlHandler {}
}

use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, HashSet},
    rc::{Rc, Weak},
};

use cxx_qt_lib::QList;
use cxx_qt_lib_shoop::{qobject::AsQObject, qpointer::QPointerQObject};
use ffi::*;

use crate::{lua_callback::LuaCallback, lua_engine::LuaEngineImpl};

pub struct SessionControlHandlerLuaTarget {
    pub structured_loop_widget_references: BTreeMap<(i64, i64), cxx::UniquePtr<QPointerQObject>>,
    pub structured_track_control_widget_references: BTreeMap<i64, cxx::UniquePtr<QPointerQObject>>,
    pub weak_self: Weak<RefCell<SessionControlHandlerLuaTarget>>,
    pub callbacks: Vec<Rc<Box<dyn LuaCallback>>>,
    pub selected_loops: Vec<*mut QObject>,
    pub maybe_targeted_loop: Option<*mut QObject>,
    pub session: *mut QObject,
    pub global_state_registry: *mut QObject,
}

pub struct SessionControlHandlerRust {
    pub loop_widget_references: QList_QVariant,
    pub track_control_widget_references: QList_QVariant,
    pub lua_target: Rc<RefCell<SessionControlHandlerLuaTarget>>,
}

impl Default for SessionControlHandlerRust {
    fn default() -> Self {
        let target = Rc::new(RefCell::new(SessionControlHandlerLuaTarget {
            structured_loop_widget_references: BTreeMap::new(),
            structured_track_control_widget_references: BTreeMap::new(),
            weak_self: std::rc::Weak::new(),
            callbacks: Vec::default(),
            selected_loops: Vec::default(),
            maybe_targeted_loop: None,
            session: std::ptr::null_mut(),
            global_state_registry: std::ptr::null_mut(),
        }));
        let weak = Rc::downgrade(&target);
        target.borrow_mut().weak_self = weak;

        Self {
            loop_widget_references: QList::default(),
            track_control_widget_references: QList::default(),
            lua_target: target,
        }
    }
}

impl AsQObject for ffi::SessionControlHandler {
    unsafe fn mut_qobject_ptr(&mut self) -> *mut ffi::QObject {
        ffi::session_control_handler_qobject_from_ptr(self as *mut Self)
    }

    unsafe fn ref_qobject_ptr(&self) -> *const ffi::QObject {
        ffi::session_control_handler_qobject_from_ref(self) as *const ffi::QObject
    }
}

impl cxx_qt::Constructor<(*mut ffi::QObject,)> for ffi::SessionControlHandler {
    type BaseArguments = (*mut ffi::QObject,); // Will be passed to the base class constructor
    type InitializeArguments = (); // Will be passed to the "initialize" function
    type NewArguments = (*mut ffi::QObject,); // Will be passed to the "new" function

    fn route_arguments(
        args: (*mut ffi::QObject,),
    ) -> (
        Self::NewArguments,
        Self::BaseArguments,
        Self::InitializeArguments,
    ) {
        (args, args, ())
    }

    fn new(_parent: (*mut ffi::QObject,)) -> SessionControlHandlerRust {
        SessionControlHandlerRust::default()
    }

    fn initialize(self: std::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        ffi::SessionControlHandler::initialize_impl(self);
    }
}

impl cxx_qt::Constructor<()> for ffi::SessionControlHandler {
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

    fn new(_args: ()) -> SessionControlHandlerRust {
        SessionControlHandlerRust::default()
    }

    fn initialize(self: std::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        ffi::SessionControlHandler::initialize_impl(self);
    }
}
