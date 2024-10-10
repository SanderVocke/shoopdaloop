use common::logging::macros::*;
shoop_log_unit!("Frontend.TestPort");

pub mod constants {
    use crate::cxx_qt_shoop::qobj_signature_port;

    pub const INVOKABLE_DETERMINE_CONNECTIONS_STATE: &str
        = qobj_signature_port::constants::INVOKABLE_DETERMINE_CONNECTIONS_STATE;

    pub const PROP_DATA_TYPE: &str = qobj_signature_port::constants::PROP_DATA_TYPE;
    pub const SIGNAL_DATA_TYPE_CHANGED: &str = qobj_signature_port::constants::SIGNAL_DATA_TYPE_CHANGED;

    pub const PROP_DIRECTION: &str = qobj_signature_port::constants::PROP_DIRECTION;
    pub const SIGNAL_DIRECTION_CHANGED: &str = qobj_signature_port::constants::SIGNAL_DIRECTION_CHANGED;

    pub const PROP_NAME: &str = qobj_signature_port::constants::PROP_NAME;
    pub const SIGNAL_NAME_CHANGED: &str = qobj_signature_port::constants::SIGNAL_NAME_CHANGED;

    pub const PROP_INITIALIZED: &str = qobj_signature_port::constants::PROP_INITIALIZED;
    pub const SIGNAL_INITIALIZED_CHANGED: &str = qobj_signature_port::constants::SIGNAL_INITIALIZED_CHANGED;

    pub const PROP_CONNECTIONS_STATE: &str = "connectionsState";
    pub const SIGNAL_CONNECTIONS_STATE_CHANGED: &str = "connectionsStateChanged()";

    pub const SIGNAL_EXTERNAL_CONNECTION_MADE: &str = "externalConnectionMade(QString)";
}

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        include!("cxx-qt-lib-shoop/qquickitem.h");
        type QQuickItem = crate::cxx_qt_lib_shoop::qquickitem::QQuickItem;
        type QObject = crate::cxx_qt_lib_shoop::qobject::QObject;
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qmap.h");
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib-shoop/qjsonobject.h");
        type QJsonObject = crate::cxx_qt_lib_shoop::qjsonobject::QJsonObject;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[base = QQuickItem]
        #[qproperty(QMap_QString_QVariant, connections_state)]
        #[qproperty(i32, data_type)]
        #[qproperty(i32, direction)]
        #[qproperty(QString, name)]
        #[qproperty(bool, initialized)]
        type TestPort = super::TestPortRust;

        #[qinvokable]
        pub fn determine_connections_state(self: Pin<&mut TestPort>) -> QMap_QString_QVariant;

        #[qinvokable]
        pub fn connect_external_port(self: Pin<&mut TestPort>, name : QString) -> bool;

        #[qsignal]
        pub fn external_connection_made(self: Pin<&mut TestPort>, port : QString);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");

        #[rust_name="qquickitem_from_ref_test_port"]
        unsafe fn qquickitemFromRef(obj : &TestPort) -> &QQuickItem;
        #[rust_name="qquickitem_from_ptr_test_port"]
        unsafe fn qquickitemFromPtr(obj : *mut TestPort) -> *mut QQuickItem;

        include!("cxx-qt-shoop/make_unique.h");
        #[rust_name = "make_unique_test_port"]
        fn make_unique() -> UniquePtr<TestPort>;
    }

    impl cxx_qt::Constructor<(*mut QQuickItem,), NewArguments=()> for TestPort {}
    impl cxx_qt::Constructor<(), NewArguments=()> for TestPort {}
}

pub use ffi::make_unique_test_port as make_unique;
pub use ffi::TestPort;
use backend_bindings::PortDataType;
use backend_bindings::PortDirection;
use crate::cxx_qt_lib_shoop::qquickitem::{AsQQuickItem, IsQQuickItem};
use ffi::*;

pub struct TestPortRust {
    pub connections_state : QMap_QString_QVariant,
    pub data_type: i32,
    pub direction: i32,
    pub name: QString,
    pub initialized: bool,
    pub connect_external_port_return_val: bool,
}

impl Default for TestPortRust {
    fn default() -> TestPortRust {
        TestPortRust {
            connections_state : QMap_QString_QVariant::default(),
            data_type : PortDataType::Audio as i32,
            direction : PortDirection::Input as i32,
            name: QString::from("unknown"),
            initialized: false,
            connect_external_port_return_val: true,
        }
    }
}

impl cxx_qt::Constructor<(*mut QQuickItem,)> for TestPort {
    type BaseArguments = (*mut QQuickItem,); // Will be passed to the base class constructor
    type InitializeArguments = (); // Will be passed to the "initialize" function
    type NewArguments = (); // Will be passed to the "new" function

    fn route_arguments(args: (*mut QQuickItem,)) -> (
        Self::NewArguments,
        Self::BaseArguments,
        Self::InitializeArguments
    ) { ((), args, ()) }

    fn new(_args : ()) -> TestPortRust { TestPortRust::default() }
}

impl cxx_qt::Constructor<()> for TestPort {
    type BaseArguments = (); // Will be passed to the base class constructor
    type InitializeArguments = (); // Will be passed to the "initialize" function
    type NewArguments = (); // Will be passed to the "new" function

    fn route_arguments(_args: ()) -> (
        Self::NewArguments,
        Self::BaseArguments,
        Self::InitializeArguments
    ) { ((), (), ()) }

    fn new(_args : ()) -> TestPortRust { TestPortRust::default() }
}

impl AsQQuickItem for TestPort {
    unsafe fn mut_qquickitem_ptr (&mut self) -> *mut QQuickItem { ffi::qquickitem_from_ptr_test_port(self as *mut Self) }
    unsafe fn ref_qquickitem_ptr (& self) -> *const QQuickItem { ffi::qquickitem_from_ref_test_port(self) as *const QQuickItem }
}

impl IsQQuickItem for TestPort {}