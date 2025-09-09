#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");
        type QQuickItem = cxx_qt_lib_shoop::qquickitem::QQuickItem;

        include!("cxx-qt-lib-shoop/qobject.h");
        type QObject = cxx_qt_lib_shoop::qobject::QObject;

        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qmap.h");
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QQuickItem]
        type LuaEngine = super::LuaEngineRust;

        #[qinvokable]
        pub fn evaluate(
            self: &LuaEngine,
            code: QString,
            maybe_script_name: QVariant,
            sandboxed: bool,
        ) -> QVariant;

        #[qinvokable]
        pub fn execute(
            self: &LuaEngine,
            code: QString,
            maybe_script_name: QVariant,
            sandboxed: bool,
        );

        #[qinvokable]
        pub fn ensure_engine_destroyed(self: Pin<&mut LuaEngine>);

        #[qinvokable]
        pub fn create_qt_to_lua_callback_fn(
            self: Pin<&mut LuaEngine>,
            name: QString,
            code: QString,
        ) -> *mut WrappedLuaCallback;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/make_unique.h");
        #[rust_name = "make_unique_lua_engine"]
        fn make_unique() -> UniquePtr<LuaEngine>;

        include!("cxx-qt-lib-shoop/qquickitem.h");

        #[rust_name = "qquickitem_from_ref_lua_engine"]
        unsafe fn qquickitemFromRef(obj: &LuaEngine) -> &QQuickItem;

        #[rust_name = "qquickitem_from_ptr_lua_engine"]
        unsafe fn qquickitemFromPtr(obj: *mut LuaEngine) -> *mut QQuickItem;

        include!("cxx-qt-lib-shoop/qobject.h");

        #[rust_name = "from_qobject_ref_lua_engine"]
        unsafe fn fromQObjectRef(obj: &QObject, output: *mut *const LuaEngine);

        #[rust_name = "from_qobject_mut_lua_engine"]
        unsafe fn fromQObjectMut(obj: Pin<&mut QObject>, output: *mut *mut LuaEngine);

        include!("cxx-qt-lib-shoop/register_qml_type.h");
        #[rust_name = "register_qml_type_lua_engine"]
        unsafe fn register_qml_type(
            inference_example: *mut LuaEngine,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );
    }

    impl cxx_qt::Constructor<(*mut QQuickItem,), NewArguments = (*mut QQuickItem,)> for LuaEngine {}
    impl cxx_qt::Constructor<(), NewArguments = ()> for LuaEngine {}

    unsafe extern "RustQt" {
        #[qobject]
        type WrappedLuaCallback = super::WrappedLuaCallbackRust;

        #[qinvokable]
        pub fn call(self: Pin<&mut WrappedLuaCallback>);

        #[qinvokable]
        pub fn call_with_arg(self: Pin<&mut WrappedLuaCallback>, arg: QVariant);

        #[qinvokable]
        pub fn call_and_delete(self: Pin<&mut WrappedLuaCallback>);

        #[qinvokable]
        pub fn delete_later(self: Pin<&mut WrappedLuaCallback>);

        #[qinvokable]
        pub fn call_with_stored_arg(self: Pin<&mut WrappedLuaCallback>);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/make_raw.h");
        #[rust_name = "make_raw_wrapped_lua_callback"]
        unsafe fn make_raw() -> *mut WrappedLuaCallback;

        #[rust_name = "make_raw_wrapped_lua_callback_with_parent"]
        unsafe fn make_raw_with_one_arg(parent: *mut QObject) -> *mut WrappedLuaCallback;

        include!("cxx-qt-lib-shoop/make_unique.h");
        #[rust_name = "make_unique_wrapped_lua_callback"]
        unsafe fn make_unique() -> UniquePtr<WrappedLuaCallback>;

        include!("cxx-qt-lib-shoop/qobject.h");

        #[rust_name = "wrapped_lua_callback_qobject_from_ptr"]
        unsafe fn qobjectFromPtr(obj: *mut WrappedLuaCallback) -> *mut QObject;

        #[rust_name = "wrapped_lua_callback_qobject_from_ref"]
        fn qobjectFromRef(obj: &WrappedLuaCallback) -> &QObject;
    }
}

use crate::lua_engine::LuaEngine as WrappedLuaEngine;
use cxx_qt_lib_shoop::qobject::AsQObject;
use ffi::*;
use std::cell::RefCell;
use std::sync::Weak;

pub struct LuaEngineRust {
    pub engine: Option<WrappedLuaEngine>,
}

impl Default for LuaEngineRust {
    fn default() -> Self {
        Self { engine: None }
    }
}

impl cxx_qt::Constructor<(*mut QQuickItem,)> for LuaEngine {
    type BaseArguments = (*mut QQuickItem,); // Will be passed to the base class constructor
    type InitializeArguments = (); // Will be passed to the "initialize" function
    type NewArguments = (*mut QQuickItem,); // Will be passed to the "new" function

    fn route_arguments(
        args: (*mut QQuickItem,),
    ) -> (
        Self::NewArguments,
        Self::BaseArguments,
        Self::InitializeArguments,
    ) {
        (args, args, ())
    }

    fn new(_parent: (*mut QQuickItem,)) -> LuaEngineRust {
        LuaEngineRust::default()
    }

    fn initialize(self: std::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        LuaEngine::initialize_impl(self);
    }
}

impl cxx_qt::Constructor<()> for LuaEngine {
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

    fn new(_args: ()) -> LuaEngineRust {
        LuaEngineRust::default()
    }

    fn initialize(self: std::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        LuaEngine::initialize_impl(self);
    }
}

impl cxx_qt_lib_shoop::qquickitem::AsQQuickItem for LuaEngine {
    unsafe fn mut_qquickitem_ptr(&mut self) -> *mut QQuickItem {
        qquickitem_from_ptr_lua_engine(self as *mut Self)
    }
    unsafe fn ref_qquickitem_ptr(&self) -> *const QQuickItem {
        qquickitem_from_ref_lua_engine(self) as *const QQuickItem
    }
}

impl cxx_qt_lib_shoop::qobject::FromQObject for LuaEngine {
    unsafe fn ptr_from_qobject_ref(obj: &cxx_qt_lib_shoop::qobject::QObject) -> *const Self {
        let mut output: *const Self = std::ptr::null();
        from_qobject_ref_lua_engine(obj, &mut output as *mut *const Self);
        output
    }

    unsafe fn ptr_from_qobject_mut(
        obj: std::pin::Pin<&mut cxx_qt_lib_shoop::qobject::QObject>,
    ) -> *mut Self {
        let mut output: *mut Self = std::ptr::null_mut();
        from_qobject_mut_lua_engine(obj, &mut output as *mut *mut Self);
        output
    }
}

impl cxx_qt_lib_shoop::qquickitem::IsQQuickItem for LuaEngine {}

pub struct RustToLuaCallback {
    pub callback: mlua::Function,
    pub weak_lua: Weak<mlua::Lua>,
}

pub struct WrappedLuaCallbackRust {
    pub callback: RefCell<Option<RustToLuaCallback>>,
    pub stored_arg: mlua::MultiValue,
}

impl Default for WrappedLuaCallbackRust {
    fn default() -> Self {
        Self {
            callback: RefCell::new(None),
            stored_arg: mlua::MultiValue::new(),
        }
    }
}

impl AsQObject for ffi::WrappedLuaCallback {
    unsafe fn mut_qobject_ptr(&mut self) -> *mut ffi::QObject {
        ffi::wrapped_lua_callback_qobject_from_ptr(self as *mut Self)
    }

    unsafe fn ref_qobject_ptr(&self) -> *const ffi::QObject {
        ffi::wrapped_lua_callback_qobject_from_ref(self) as *const ffi::QObject
    }
}
