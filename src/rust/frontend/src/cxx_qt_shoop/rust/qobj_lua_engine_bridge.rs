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
}

use crate::lua_engine::LuaEngine as WrappedLuaEngine;
use ffi::*;

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
