use std::collections::HashSet;

use backend_bindings::DecoupledMidiPort;
use common::logging::macros::*;
shoop_log_unit!("Frontend.MidiControlPort");

#[cxx_qt::bridge]
pub mod ffi {

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");
        type QQuickItem = cxx_qt_lib_shoop::qquickitem::QQuickItem;
        type QObject = cxx_qt_lib_shoop::qobject::QObject;

        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qlist.h");
        type QList_QVariant = cxx_qt_lib::QList<cxx_qt_lib::QVariant>;
        type QList_f32 = cxx_qt_lib::QList<f32>;
        type QList_u8 = cxx_qt_lib::QList<u8>;
        type QList_QString = cxx_qt_lib::QList<QString>;

        include!("cxx-qt-lib/qmap.h");
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;

        include!("cxx-qt-lib-shoop/qsharedpointer_qobject.h");
        type QSharedPointer_QObject =
            cxx_qt_lib_shoop::qsharedpointer_qobject::QSharedPointer_QObject;

        include!("cxx-qt-lib-shoop/qmetatype.h");
        #[rust_name = "midi_control_port_metatype_name"]
        unsafe fn meta_type_name(obj: *mut MidiControlPort) -> Result<String>;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QQuickItem]
        #[qproperty(*mut QObject, backend)]
        #[qproperty(bool, initialized)]
        #[qproperty(QList_QString, autoconnect_regexes)]
        #[qproperty(QString, name_hint)]
        #[qproperty(QString, name)]
        #[qproperty(i32, direction)]
        #[qproperty(bool, may_open)]
        #[qproperty(i32, send_rate_limit_hz)]
        type MidiControlPort = super::MidiControlPortRust;

        pub fn initialize_impl(self: Pin<&mut MidiControlPort>);

        #[qinvokable]
        pub fn get_connections_state(self: Pin<&mut MidiControlPort>) -> QMap_QString_QVariant;

        #[qinvokable]
        pub fn connect_external_port(self: Pin<&mut MidiControlPort>, name: QString);

        #[qinvokable]
        pub fn disconnect_external_port(self: Pin<&mut MidiControlPort>, name: QString);

        #[qinvokable]
        pub fn update_send_queue(self: Pin<&mut MidiControlPort>);

        #[qinvokable]
        pub fn queue_send_msg(self: Pin<&mut MidiControlPort>, msg: QList_u8);

        #[qinvokable]
        pub fn autoconnect_update(self: Pin<&mut MidiControlPort>);

        #[qinvokable]
        pub fn poll(self: Pin<&mut MidiControlPort>);

        #[qinvokable]
        pub fn maybe_initialize(self: Pin<&mut MidiControlPort>);

        #[qsignal]
        pub fn msg_received(self: Pin<&mut MidiControlPort>, msg: QList_u8);

        #[qsignal]
        pub fn detected_external_autoconnect_partner_while_closed(self: Pin<&mut MidiControlPort>);

        #[qsignal]
        pub fn connected(self: Pin<&mut MidiControlPort>);

        #[qinvokable]
        pub fn send_detected_external_autoconnect_partner_while_closed(
            self: Pin<&mut MidiControlPort>,
        );

        #[qinvokable]
        pub fn send_connected(self: Pin<&mut MidiControlPort>);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");

        #[rust_name = "qquickitem_from_ref_midi_control_port"]
        unsafe fn qquickitemFromRef(obj: &MidiControlPort) -> &QQuickItem;

        #[rust_name = "qquickitem_from_ptr_midi_control_port"]
        unsafe fn qquickitemFromPtr(obj: *mut MidiControlPort) -> *mut QQuickItem;

        include!("cxx-qt-lib-shoop/make_unique.h");
        #[rust_name = "make_unique_midi_control_port"]
        fn make_unique() -> UniquePtr<MidiControlPort>;

        include!("cxx-qt-lib-shoop/make_raw.h");
        #[rust_name = "make_raw_midi_control_port"]
        fn make_raw() -> *mut MidiControlPort;

        include!("cxx-qt-lib-shoop/cast_ptr.h");
        #[rust_name = "qobject_to_midi_control_port_ptr"]
        unsafe fn cast_qobject_ptr(obj: *mut QObject) -> *mut MidiControlPort;

        include!("cxx-qt-lib-shoop/qobject_classname.h");
        #[rust_name = "qobject_class_name_midi_control_port"]
        fn qobject_class_name(obj: &MidiControlPort) -> Result<&str>;

        include!("cxx-qt-lib-shoop/register_qml_type.h");
        #[rust_name = "register_qml_type_midi_control_port"]
        unsafe fn register_qml_type(
            inference_example: *mut MidiControlPort,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );

        include!("cxx-qt-lib-shoop/qobject.h");
        #[rust_name = "midi_control_port_qobject_from_ptr"]
        unsafe fn qobjectFromPtr(obj: *mut MidiControlPort) -> *mut QObject;

        #[rust_name = "midi_control_port_qobject_from_ref"]
        fn qobjectFromRef(obj: &MidiControlPort) -> &QObject;

        #[rust_name = "from_qobject_ref_midi_control_port"]
        unsafe fn fromQObjectRef(obj: &QObject, output: *mut *const MidiControlPort);

        #[rust_name = "from_qobject_mut_midi_control_port"]
        unsafe fn fromQObjectMut(obj: Pin<&mut QObject>, output: *mut *mut MidiControlPort);
    }

    impl cxx_qt::Constructor<(*mut QQuickItem,), NewArguments = (*mut QQuickItem,)>
        for MidiControlPort
    {
    }
    impl cxx_qt::Constructor<()> for MidiControlPort {}
}

use cxx_qt_lib_shoop::qquickitem::IsQQuickItem;
pub use ffi::MidiControlPort;
use ffi::*;

use crate::cxx_qt_shoop::qobj_autoconnect::AutoConnect;

pub struct MidiControlPortRust {
    // Properties
    pub backend: *mut QObject,
    pub initialized: bool,
    pub autoconnect_regexes: QList_QString,
    pub name_hint: QString,
    pub name: QString,
    pub direction: i32,
    pub may_open: bool,
    pub send_rate_limit_hz: i32,

    // Other
    pub backend_port_wrapper: Option<DecoupledMidiPort>,
    pub send_timer: *mut QObject,
    pub send_queue: Vec<Vec<u8>>,
    pub active_notes: HashSet<(u8, u8)>,
    pub cc_states: Vec<Vec<Option<u8>>>,
    pub autoconnecters: Vec<cxx::UniquePtr<AutoConnect>>,
}

impl Default for MidiControlPortRust {
    fn default() -> MidiControlPortRust {
        MidiControlPortRust {
            backend: std::ptr::null_mut(),
            initialized: false,
            autoconnect_regexes: QList_QString::default(),
            name_hint: QString::default(),
            name: QString::default(),
            direction: 0,
            may_open: false,
            send_rate_limit_hz: 0,
            backend_port_wrapper: None,
            send_timer: std::ptr::null_mut(),
            send_queue: Vec::new(),
            active_notes: HashSet::new(),
            cc_states: Vec::from_iter(std::iter::repeat_n(
                Vec::from_iter(std::iter::repeat(None).take(128)), // 128 CCs
                16,                                                // 16 channels
            )),
            autoconnecters: Vec::new(),
        }
    }
}

impl cxx_qt_lib_shoop::qquickitem::AsQQuickItem for MidiControlPort {
    unsafe fn mut_qquickitem_ptr(&mut self) -> *mut QQuickItem {
        qquickitem_from_ptr_midi_control_port(self as *mut Self)
    }
    unsafe fn ref_qquickitem_ptr(&self) -> *const QQuickItem {
        qquickitem_from_ref_midi_control_port(self) as *const QQuickItem
    }
}

impl cxx_qt_lib_shoop::qobject::FromQObject for MidiControlPort {
    unsafe fn ptr_from_qobject_ref(obj: &cxx_qt_lib_shoop::qobject::QObject) -> *const Self {
        let mut output: *const Self = std::ptr::null();
        from_qobject_ref_midi_control_port(obj, &mut output as *mut *const Self);
        output
    }

    unsafe fn ptr_from_qobject_mut(
        obj: std::pin::Pin<&mut cxx_qt_lib_shoop::qobject::QObject>,
    ) -> *mut Self {
        let mut output: *mut Self = std::ptr::null_mut();
        from_qobject_mut_midi_control_port(obj, &mut output as *mut *mut Self);
        output
    }
}

impl IsQQuickItem for MidiControlPort {}

impl cxx_qt::Constructor<(*mut QQuickItem,)> for MidiControlPort {
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

    fn new(_parent: (*mut QQuickItem,)) -> MidiControlPortRust {
        MidiControlPortRust::default()
    }

    fn initialize(self: core::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        MidiControlPort::initialize_impl(self);
    }
}

impl cxx_qt::Constructor<()> for MidiControlPort {
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

    fn new(_args: ()) -> MidiControlPortRust {
        MidiControlPortRust::default()
    }

    fn initialize(self: core::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        MidiControlPort::initialize_impl(self);
    }
}
