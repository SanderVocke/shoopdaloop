use common::logging::macros::*;
use backend_bindings::Loop as BackendLoop;
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
        type QList_f32 = cxx_qt_lib::QList<f32>;

        include!("cxx-qt-lib/qmap.h");
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;

        include!("cxx-qt-lib-shoop/metatype.h");
        #[rust_name = "loop_metatype_name"]
        unsafe fn meta_type_name(obj: &Loop) -> Result<&str>;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QQuickItem]
        #[qproperty(*mut QObject, backend)]
        #[qproperty(bool, initialized)]
        #[qproperty(i32, mode)]
        #[qproperty(i32, length)]
        #[qproperty(i32, position)]
        #[qproperty(i32, next_mode)]
        #[qproperty(i32, next_transition_delay)]
        #[qproperty(*mut QObject, sync_source)]
        #[qproperty(QList_f32, display_peaks)]
        #[qproperty(i32, display_midi_notes_active)]
        #[qproperty(i32, display_midi_events_triggered)]
        #[qproperty(QString, instance_identifier)]
        type Loop = super::LoopRust;

        pub fn initialize_impl(self: Pin<&mut Loop>);

        #[qinvokable]
        pub fn update_on_non_gui_thread(self: Pin<&mut Loop>);

        #[qinvokable]
        pub fn update_on_gui_thread(self: Pin<&mut Loop>);

        #[qinvokable]
        pub fn queue_set_length(self: Pin<&mut Loop>, length: i32);

        #[qinvokable]
        pub fn queue_set_position(self: Pin<&mut Loop>, position: i32);

        #[qinvokable]
        pub fn get_audio_channels(self: Pin<&mut Loop>) -> QList_QVariant;

        #[qinvokable]
        pub fn get_midi_channels(self: Pin<&mut Loop>) -> QList_QVariant;

        #[qinvokable]
        pub fn transition_multiple(self: Pin<&mut Loop>,
                                   loops: QList_QVariant,
                                   to_mode: i32,
                                   maybe_cycles_delay: i32,
                                   maybe_to_sync_at_cycle: i32);
        
        #[qinvokable]
        pub fn maybe_initialize_backend(self: Pin<&mut Loop>);

        #[qsignal]
        fn cycled(self: Pin<&mut Loop>, cycle_nr: i32);

        #[qsignal]
        fn cycled_queued(self: Pin<&mut Loop>, cycle_nr: i32);

        #[qsignal]
        fn starting_update_on_non_gui_thread(self: Pin<&mut Loop>);

        #[qsignal]
        fn mode_changed_queued(self: Pin<&mut Loop>);

        #[qsignal]
        fn next_mode_changed_queued(self: Pin<&mut Loop>);

        #[qsignal]
        fn length_changed_queued(self: Pin<&mut Loop>);

        #[qsignal]
        fn position_changed_queued(self: Pin<&mut Loop>);

        #[qsignal]
        fn next_transition_delay_changed_queued(self: Pin<&mut Loop>);

        #[qsignal]
        fn display_peaks_changed_queued(self: Pin<&mut Loop>);

        #[qsignal]
        fn display_midi_notes_active_changed_queued(self: Pin<&mut Loop>);

        #[qsignal]
        fn display_midi_events_triggered_changed_queued(self: Pin<&mut Loop>);
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

        include!("cxx-qt-shoop/cast_ptr.h");
        #[rust_name = "qobject_to_loop_ptr"]
        unsafe fn cast_qobject_ptr(obj : *mut QObject) -> *mut Loop;

        include!("cxx-qt-shoop/qobject_classname.h");
        #[rust_name = "qobject_class_name_loop"]
        fn qobject_class_name(obj: &Loop) -> Result<&str>;

        include!("cxx-qt-lib-shoop/qjsonobject.h");
        type QJsonObject = crate::cxx_qt_lib_shoop::qjsonobject::QJsonObject;

        include!("cxx-qt-shoop/register_qml_type.h");
        #[rust_name = "register_qml_type_loop"]
        fn register_qml_type(inference_example: &Loop,
                             module_name: &mut String,
                             version_major: i64, version_minor: i64,
                             type_name: &mut String);
    }

    impl cxx_qt::Constructor<(*mut QQuickItem,), NewArguments=(*mut QQuickItem,)> for Loop {}
    impl cxx_qt::Constructor<()> for Loop {}
}

pub use ffi::Loop;
use ffi::*;
use cxx::UniquePtr;
use crate::cxx_qt_lib_shoop::qquickitem::IsQQuickItem;
use std::sync::{Arc, Mutex};

pub struct LoopRust {
    // Properties
    pub backend: *mut QObject,
    pub initialized: bool,
    pub mode: i32,
    pub length: i32,
    pub position: i32,
    pub next_mode: i32,
    pub next_transition_delay: i32,
    pub sync_source: *mut QObject,
    pub display_peaks: ffi::QList_f32,
    pub display_midi_notes_active: i32,
    pub display_midi_events_triggered: i32,
    pub instance_identifier: QString,

    // Rust members
    pub backend_loop : Option<Arc<Mutex<BackendLoop>>>,
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
            sync_source: std::ptr::null_mut(),
            display_peaks: ffi::QList_f32::default(),
            display_midi_notes_active: 0,
            display_midi_events_triggered: 0,
            instance_identifier: QString::default(),
            backend_loop: None,
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
