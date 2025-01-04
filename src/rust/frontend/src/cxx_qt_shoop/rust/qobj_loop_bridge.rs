use common::logging::macros::*;
shoop_log_unit!("Frontend.Loop");

pub mod constants {
    pub const PROP_BACKEND: &str = "backend";
    pub const PROP_INITIALIZED: &str = "initialized";
    pub const PROP_MODE: &str = "mode";
    pub const PROP_LENGTH: &str = "length";
    pub const PROP_POSITION: &str = "position";
    pub const PROP_NEXT_MODE: &str = "nextMode";
    pub const PROP_NEXT_TRANSITION_DELAY: &str = "nextTransitionDelay";
    pub const PROP_SYNC_SOURCE: &str = "syncSource";
    pub const PROP_DISPLAY_PEAKS: &str = "displayPeaks";
    pub const PROP_DISPLAY_MIDI_NOTES_ACTIVE: &str = "displayMidiNotesActive";
    pub const PROP_DISPLAY_MIDI_EVENTS_TRIGGERED: &str = "displayMidiEventsTriggered";
    pub const PROP_INSTANCE_IDENTIFIER: &str = "instanceIdentifier";

    pub const SIGNAL_CYCLED: &str = "cycled()";
    pub const SIGNAL_CYCLED_UNSAFE: &str = "cycledUnsafe()";

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

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qlist.h");
        type QList_QVariant = cxx_qt_lib::QList<cxx_qt_lib::QVariant>;

        include!("cxx-qt-lib/qmap.h");
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;

        include!("cxx-qt-lib-shoop/invoke.h");
        #[rust_name = "invoke_with_return_variantmap"]
        unsafe fn invoke_with_return(obj: *mut QObject, method: String) -> Result<QMap_QString_QVariant>;

        include!("cxx-qt-lib-shoop/metatype.h");
        #[rust_name = "loop_metatype_name"]
        unsafe fn meta_type_name(obj: &Loop) -> Result<&str>;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[base = QQuickItem]
        #[qproperty(*mut QObject, backend)]
        #[qproperty(bool, initialized)]
        #[qproperty(i32, mode)]
        #[qproperty(i32, length)]
        #[qproperty(i32, position)]
        #[qproperty(i32, next_mode)]
        #[qproperty(i32, next_transition_delay)]
        #[qproperty(QVariant, sync_source)]
        #[qproperty(QVariant, display_peaks)]
        #[qproperty(i32, display_midi_notes_active)]
        #[qproperty(i32, display_midi_events_triggered)]
        #[qproperty(QString, instance_identifier)]
        type Loop = super::LoopRust;

        pub fn initialize_impl(self: Pin<&mut Loop>);

        #[qinvokable]
        pub fn updateOnNonGuiThread(self: Pin<&mut Loop>);

        #[qinvokable]
        pub fn updateOnGuiThread(self: Pin<&mut Loop>);

        #[qsignal]
        fn cycled(self: Pin<&mut Loop>, cycle_nr: i32);

        #[qsignal]
        fn cycled_unsafe(self: Pin<&mut Loop>, cycle_nr: i32);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");

        #[rust_name = "qquickitem_from_ref_loop"]
        unsafe fn qquickitemFromRef(obj: &Loop) -> &QQuickItem;

        #[rust_name = "qquickitem_from_ptr_loop"]
        unsafe fn qquickitemFromPtr(obj: *mut Loop) -> *mut QQuickItem;

        include!("cxx-qt-shoop/make_unique.h");
        #[rust_name = "make_unique_loop"]
        fn make_unique() -> UniquePtr<Loop>;

        include!("cxx-qt-shoop/make_raw.h");
        #[rust_name = "make_raw_loop"]
        fn make_raw() -> *mut Loop;

        include!("cxx-qt-shoop/qobject_classname.h");
        #[rust_name = "qobject_class_name_loop"]
        fn qobject_class_name(obj: &Loop) -> Result<&str>;

        include!("cxx-qt-lib-shoop/qjsonobject.h");
        type QJsonObject = crate::cxx_qt_lib_shoop::qjsonobject::QJsonObject;

        include!("cxx-qt-lib-shoop/connect.h");
        #[rust_name = "connect_to_loop"]
        unsafe fn connect(sender: *mut QQuickItem,
                          signal: String,
                          receiver: *mut Loop,
                          slot: String) -> Result<()>;

        include!("cxx-qt-shoop/register_qml_type.h");

        #[rust_name = "register_qml_type_loop"]
        fn register_qml_type(inference_example: &Loop,
                             module_name: &mut String,
                             version_major: i64, version_minor: i64,
                             type_name: &mut String);
    }

    impl cxx_qt::Constructor<(*mut QQuickItem,), NewArguments=(*mut QQuickItem,)> for Loop {}
    impl cxx_qt::Constructor<(), NewArguments=()> for Loop {}
}

pub use ffi::Loop;
use ffi::*;
use cxx::UniquePtr;
use crate::cxx_qt_lib_shoop::qquickitem::IsQQuickItem;
pub struct LoopRust {
    // Properties
    pub backend: *mut QObject,
    pub initialized: bool,
    pub mode: i32,
    pub length: i32,
    pub position: i32,
    pub next_mode: i32,
    pub next_transition_delay: i32,
    pub sync_source: QVariant,
    pub display_peaks: QVariant,
    pub display_midi_notes_active: i32,
    pub display_midi_events_triggered: i32,
    pub instance_identifier: QString,
}

impl Default for LoopRust {
    fn default() -> LoopRust {
        LoopRust {
            backend: std::ptr::null_mut(),
            initialized: false,
            mode: 0,
            length: 0,
            position: 0,
            next_mode: 0,
            next_transition_delay: 0,
            sync_source: QVariant::default(),
            display_peaks: QVariant::default(),
            display_midi_notes_active: 0,
            display_midi_events_triggered: 0,
            instance_identifier: QString::default(),
        }
    }
}

impl crate::cxx_qt_lib_shoop::qquickitem::AsQQuickItem for Loop {
    unsafe fn mut_qquickitem_ptr(&mut self) -> *mut QQuickItem { qquickitem_from_ptr_loop(self as *mut Self) }
    unsafe fn ref_qquickitem_ptr(&self) -> *const QQuickItem { qquickitem_from_ref_loop(self) as *const QQuickItem }
}

impl IsQQuickItem for Loop {}

impl cxx_qt::Constructor<(*mut QQuickItem,)> for Loop {
    type BaseArguments = (*mut QQuickItem,); // Will be passed to the base class constructor
    type InitializeArguments = (); // Will be passed to the "initialize" function
    type NewArguments = (*mut QQuickItem,); // Will be passed to the "new" function

    fn route_arguments(args: (*mut QQuickItem,)) -> (
        Self::NewArguments,
        Self::BaseArguments,
        Self::InitializeArguments
    ) {
        (args, args, ())
    }

    fn new(_parent: (*mut QQuickItem,)) -> LoopRust {
        LoopRust::default()
    }

    fn initialize(self: core::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        Loop::initialize_impl(self);
    }
}

impl cxx_qt::Constructor<()> for Loop {
    type BaseArguments = (); // Will be passed to the base class constructor
    type InitializeArguments = (); // Will be passed to the "initialize" function
    type NewArguments = (); // Will be passed to the "new" function

    fn route_arguments(_args: ()) -> (
        Self::NewArguments,
        Self::BaseArguments,
        Self::InitializeArguments
    ) {
        ((), (), ())
    }

    fn new(_args: ()) -> LoopRust {
        LoopRust::default()
    }

    fn initialize(self: core::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        Loop::initialize_impl(self);
    }
}
