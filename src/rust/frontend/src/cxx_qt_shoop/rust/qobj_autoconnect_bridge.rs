use common::logging::macros::*;
shoop_log_unit!("Frontend.AutoConnect");

pub mod constants {
    pub const PROP_INTERNAL_PORT: &str = "internal_port";
    pub const PROP_CONNECTTOPORTREGEX: &str = "connect_to_port_regex";
    pub const PROP_CLOSED: &str = "closed";

    pub const SIGNAL_INTERNAL_PORT_CHANGED: &str = "internal_portChanged()";
    pub const SIGNAL_CONNECTTOPORTREGEX_CHANGED: &str = "connect_to_port_regexChanged()";
    pub const SIGNAL_CLOSED_CHANGED: &str = "closedChanged()";

    pub const SIGNAL_CONNECTED: &str = "connected()";
    pub const SIGNAL_ONLY_EXTERNAL_FOUND: &str = "only_external_found()";

    pub const INVOKABLE_UPDATE: &str = "update()";
}

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");
        type QQuickItem = crate::cxx_qt_lib_shoop::qquickitem::QQuickItem;
        type QObject = crate::cxx_qt_lib_shoop::qobject::QObject;

        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib-shoop/qtimer.h");
        type QTimer = crate::cxx_qt_lib_shoop::qtimer::QTimer;

        include!("cxx-qt-lib/qmap.h");
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qlist.h");
        type QList_QVariant = cxx_qt_lib::QList<cxx_qt_lib::QVariant>;

        include!("cxx-qt-lib-shoop/metatype.h");
        #[rust_name = "autoconnect_metatype_name"]
        unsafe fn meta_type_name(obj: &AutoConnect) -> Result<&str>;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[base = QQuickItem]
        #[qproperty(*mut QObject, internal_port)]
        #[qproperty(QString, connect_to_port_regex)]
        #[qproperty(bool, is_closed)]
        type AutoConnect = super::AutoConnectRust;

        pub fn initialize_impl(self: Pin<&mut AutoConnect>);

        #[qinvokable]
        pub fn update(self: Pin<&mut AutoConnect>);

        #[inherit]
        #[qsignal]
        #[cxx_name = "parentChanged"]
        unsafe fn parent_changed(self: Pin<&mut AutoConnect>, parent: *mut QQuickItem);

        #[qsignal]
        fn connected(self: Pin<&mut AutoConnect>);

        #[qsignal]
        fn only_external_found(self: Pin<&mut AutoConnect>);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");

        #[rust_name = "qquickitem_from_ref_autoconnect"]
        unsafe fn qquickitemFromRef(obj: &AutoConnect) -> &QQuickItem;

        #[rust_name = "qquickitem_from_ptr_autoconnect"]
        unsafe fn qquickitemFromPtr(obj: *mut AutoConnect) -> *mut QQuickItem;

        include!("cxx-qt-shoop/make_unique.h");
        #[rust_name = "make_unique_autoconnect"]
        fn make_unique() -> UniquePtr<AutoConnect>;

        include!("cxx-qt-shoop/make_raw.h");
        #[rust_name = "make_raw_autoconnect"]
        fn make_raw() -> *mut AutoConnect;

        include!("cxx-qt-shoop/qobject_classname.h");
        #[rust_name = "qobject_class_name_autoconnect"]
        fn qobject_class_name(obj: &AutoConnect) -> Result<&str>;

        include!("cxx-qt-lib-shoop/qjsonobject.h");
        type QJsonObject = crate::cxx_qt_lib_shoop::qjsonobject::QJsonObject;

        include!("cxx-qt-shoop/register_qml_type.h");
        #[rust_name = "register_qml_type_autoconnect"]
        fn register_qml_type(
            inference_example: &AutoConnect,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );
    }

    impl cxx_qt::Constructor<(*mut QQuickItem,), NewArguments = (*mut QQuickItem,)> for AutoConnect {}
    impl cxx_qt::Constructor<(), NewArguments = ()> for AutoConnect {}
}

use crate::cxx_qt_lib_shoop::qquickitem::IsQQuickItem;
use crate::cxx_qt_shoop::fn_find_backend_wrapper;
use crate::cxx_qt_shoop::qobj_find_parent_item::FindParentItem;
use cxx::UniquePtr;
pub use ffi::AutoConnect;
use ffi::*;
pub struct AutoConnectRust {
    // Properties
    pub internal_port: *mut QObject,
    pub connect_to_port_regex: QString,
    pub is_closed: bool,
    // Private
    pub find_backend_wrapper: UniquePtr<FindParentItem>,
    pub timer: *mut QTimer,
}

impl Default for AutoConnectRust {
    fn default() -> AutoConnectRust {
        AutoConnectRust {
            internal_port: std::ptr::null_mut(),
            connect_to_port_regex: QString::default(),
            is_closed: false,
            find_backend_wrapper: fn_find_backend_wrapper::create_find_parent_backend_wrapper(),
            timer: std::ptr::null_mut(),
        }
    }
}

impl crate::cxx_qt_lib_shoop::qquickitem::AsQQuickItem for AutoConnect {
    unsafe fn mut_qquickitem_ptr(&mut self) -> *mut QQuickItem {
        qquickitem_from_ptr_autoconnect(self as *mut Self)
    }
    unsafe fn ref_qquickitem_ptr(&self) -> *const QQuickItem {
        qquickitem_from_ref_autoconnect(self) as *const QQuickItem
    }
}

impl IsQQuickItem for AutoConnect {}

impl cxx_qt::Constructor<(*mut QQuickItem,)> for AutoConnect {
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

    fn new(_parent: (*mut QQuickItem,)) -> AutoConnectRust {
        AutoConnectRust::default()
    }

    fn initialize(self: core::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        AutoConnect::initialize_impl(self);
    }
}

impl cxx_qt::Constructor<()> for AutoConnect {
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

    fn new(_args: ()) -> AutoConnectRust {
        AutoConnectRust::default()
    }

    fn initialize(self: core::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        AutoConnect::initialize_impl(self);
    }
}
