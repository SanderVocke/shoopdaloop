use crate::cxx_qt_lib_shoop::qsharedpointer_qobject::QSharedPointer_QObject;
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
    pub const INVOKABLE_TRANSITION: &str =
        "transition(::std::int32_t,::std::int32_t,::std::int32_t)";
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

        include!("cxx-qt-lib-shoop/qsharedpointer_qobject.h");
        type QSharedPointer_QObject =
            crate::cxx_qt_lib_shoop::qsharedpointer_qobject::QSharedPointer_QObject;

        include!("cxx-qt-lib-shoop/metatype.h");
        #[rust_name = "loop_gui_metatype_name"]
        unsafe fn meta_type_name(obj: &LoopGui) -> Result<&str>;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QQuickItem]
        #[qproperty(*mut QObject, backend, READ, WRITE=set_backend, NOTIFY=backend_changed)]
        #[qproperty(bool, initialized, READ, NOTIFY=initialized_changed)]
        #[qproperty(i32, mode, READ, NOTIFY=mode_changed)]
        #[qproperty(i32, length, READ, NOTIFY=length_changed)]
        #[qproperty(i32, position, READ, NOTIFY=position_changed)]
        #[qproperty(i32, next_mode, READ, NOTIFY=next_mode_changed)]
        #[qproperty(i32, next_transition_delay, READ, NOTIFY=next_transition_delay_changed)]
        #[qproperty(*mut QObject, sync_source, READ, WRITE=set_sync_source, NOTIFY=sync_source_changed)]
        #[qproperty(*mut QObject, backend_loop_wrapper, READ=get_backend_loop_wrapper)]
        #[qproperty(QList_f32, display_peaks, READ, NOTIFY)]
        #[qproperty(i32, display_midi_notes_active, READ, NOTIFY)]
        #[qproperty(i32, display_midi_events_triggered, READ, NOTIFY)]
        #[qproperty(QString, instance_identifier, READ, WRITE=set_instance_identifier, NOTIFY=instance_identifier_changed)]
        #[qproperty(i32, cycle_nr, READ, NOTIFY)]
        type LoopGui = super::LoopGuiRust;

        pub fn initialize_impl(self: Pin<&mut LoopGui>);

        #[qinvokable]
        pub fn queue_set_length(self: Pin<&mut LoopGui>, length: i32);

        #[qinvokable]
        pub fn queue_set_position(self: Pin<&mut LoopGui>, position: i32);

        #[qinvokable]
        pub fn get_audio_channels(self: Pin<&mut LoopGui>) -> QList_QVariant;

        #[qinvokable]
        pub fn get_midi_channels(self: Pin<&mut LoopGui>) -> QList_QVariant;

        #[qinvokable]
        pub fn transition_multiple(
            self: Pin<&mut LoopGui>,
            loops: QList_QVariant,
            to_mode: i32,
            maybe_cycles_delay: i32,
            maybe_to_sync_at_cycle: i32,
        );

        #[qinvokable]
        pub fn transition(
            self: Pin<&mut LoopGui>,
            to_mode: i32,
            maybe_cycles_delay: i32,
            maybe_to_sync_at_cycle: i32,
        );

        #[qinvokable]
        pub fn clear(self: Pin<&mut LoopGui>, length: i32);

        #[qinvokable]
        pub fn close(self: Pin<&mut LoopGui>);

        #[qinvokable]
        pub fn adopt_ringbuffers(
            self: Pin<&mut LoopGui>,
            maybe_reverse_start_cycle: QVariant,
            maybe_cycles_length: QVariant,
            maybe_go_to_cycle: QVariant,
            go_to_mode: i32,
        );

        #[qinvokable]
        pub fn update_backend_sync_source(self: Pin<&mut LoopGui>);

        #[qinvokable]
        pub fn on_backend_state_changed(
            self: Pin<&mut LoopGui>,
            mode: i32,
            length: i32,
            position: i32,
            next_mode: i32,
            next_transition_delay: i32,
            cycle_nr: i32,
        );

        #[qinvokable]
        pub unsafe fn set_sync_source(self: Pin<&mut LoopGui>, sync_source: *mut QObject);

        #[qinvokable]
        pub unsafe fn set_backend(self: Pin<&mut LoopGui>, backend: *mut QObject);

        #[qinvokable]
        pub unsafe fn set_initialized(self: Pin<&mut LoopGui>, initialized: bool);

        #[qinvokable]
        pub fn set_instance_identifier(self: Pin<&mut LoopGui>, instance_identifier: QString);

        #[qinvokable]
        pub fn get_backend_loop_wrapper(self: Pin<&mut LoopGui>) -> *mut QObject;

        #[qsignal]
        #[cxx_name = "backendChanged"]
        pub unsafe fn backend_changed(self: Pin<&mut LoopGui>, backend: *mut QObject);

        #[qsignal]
        #[cxx_name = "syncSourceChanged"]
        pub unsafe fn sync_source_changed(self: Pin<&mut LoopGui>, sync_source: *mut QObject);

        #[qsignal]
        #[cxx_name = "instanceIdentifierChanged"]
        pub unsafe fn instance_identifier_changed(
            self: Pin<&mut LoopGui>,
            instance_identifier: QString,
        );

        #[qsignal]
        #[cxx_name = "initializedChanged"]
        pub unsafe fn initialized_changed(self: Pin<&mut LoopGui>, initialized: bool);

        #[qsignal]
        #[cxx_name = "nextModeChanged"]
        pub unsafe fn next_mode_changed(self: Pin<&mut LoopGui>);

        #[qsignal]
        #[cxx_name = "modeChanged"]
        pub unsafe fn mode_changed(self: Pin<&mut LoopGui>);

        #[qsignal]
        #[cxx_name = "lengthChanged"]
        pub unsafe fn length_changed(self: Pin<&mut LoopGui>);

        #[qsignal]
        #[cxx_name = "positionChanged"]
        pub unsafe fn position_changed(self: Pin<&mut LoopGui>);

        #[qsignal]
        #[cxx_name = "nextTransitionDelayChanged"]
        pub unsafe fn next_transition_delay_changed(self: Pin<&mut LoopGui>);

        #[qsignal]
        #[cxx_name = "backendClose"]
        pub unsafe fn backend_close(self: Pin<&mut LoopGui>);

        #[qsignal]
        fn cycled(self: Pin<&mut LoopGui>, cycle_nr: i32);

        // The following signals are to internally connect to the back-end object
        // which lives on another thread.
        #[qsignal]
        fn backend_set_position(self: Pin<&mut LoopGui>, position: i32);
        #[qsignal]
        fn backend_set_length(self: Pin<&mut LoopGui>, length: i32);
        #[qsignal]
        fn backend_set_instance_identifier(self: Pin<&mut LoopGui>, instance_identifier: QString);
        #[qsignal]
        fn backend_clear(self: Pin<&mut LoopGui>, length: i32);
        #[qsignal]
        fn backend_transition(
            self: Pin<&mut LoopGui>,
            to_mode: i32,
            maybe_cycles_delay: i32,
            maybe_to_sync_at_cycle: i32,
        );
        #[qsignal]
        fn backend_adopt_ringbuffers(
            self: Pin<&mut LoopGui>,
            maybe_reverse_start_cycle: QVariant,
            maybe_cycles_length: QVariant,
            maybe_go_to_cycle: QVariant,
            go_to_mode: i32,
        );
        #[qsignal]
        pub fn backend_transition_multiple(
            self: Pin<&mut LoopGui>,
            loops: QList_QVariant,
            to_mode: i32,
            maybe_cycles_delay: i32,
            maybe_to_sync_at_cycle: i32,
        );
        #[qsignal]
        pub fn backend_set_sync_source(self: Pin<&mut LoopGui>, sync_source: QVariant);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");

        #[rust_name = "qquickitem_from_ref_loop"]
        unsafe fn qquickitemFromRef(obj: &LoopGui) -> &QQuickItem;

        #[rust_name = "qquickitem_from_ptr_loop"]
        unsafe fn qquickitemFromPtr(obj: *mut LoopGui) -> *mut QQuickItem;

        include!("cxx-qt-shoop/make_unique.h");
        #[rust_name = "make_unique_loop"]
        fn make_unique() -> UniquePtr<LoopGui>;

        include!("cxx-qt-shoop/make_raw.h");
        #[rust_name = "make_raw_loop_gui"]
        fn make_raw() -> *mut LoopGui;

        include!("cxx-qt-shoop/cast_ptr.h");
        #[rust_name = "qobject_to_loop_ptr"]
        unsafe fn cast_qobject_ptr(obj: *mut QObject) -> *mut LoopGui;

        include!("cxx-qt-shoop/qobject_classname.h");
        #[rust_name = "qobject_class_name_loop"]
        fn qobject_class_name(obj: &LoopGui) -> Result<&str>;

        include!("cxx-qt-lib-shoop/qjsonobject.h");
        type QJsonObject = crate::cxx_qt_lib_shoop::qjsonobject::QJsonObject;

        include!("cxx-qt-shoop/register_qml_type.h");
        #[rust_name = "register_qml_type_loop"]
        fn register_qml_type(
            inference_example: &LoopGui,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );

        include!("cxx-qt-lib-shoop/qobject.h");
        #[rust_name = "loop_gui_qobject_from_ptr"]
        unsafe fn qobjectFromPtr(obj: *mut LoopGui) -> *mut QObject;

        #[rust_name = "loop_gui_qobject_from_ref"]
        fn qobjectFromRef(obj: &LoopGui) -> &QObject;
    }

    impl cxx_qt::Constructor<(*mut QQuickItem,), NewArguments = (*mut QQuickItem,)> for LoopGui {}
    impl cxx_qt::Constructor<()> for LoopGui {}
}

use crate::cxx_qt_lib_shoop::qquickitem::IsQQuickItem;
pub use ffi::LoopGui;
use ffi::*;

pub struct LoopGuiRust {
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
    pub cycle_nr: i32,
    pub backend_loop_wrapper: cxx::UniquePtr<QSharedPointer_QObject>,
}

impl Default for LoopGuiRust {
    fn default() -> LoopGuiRust {
        LoopGuiRust {
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
            instance_identifier: QString::from("unknown"),
            cycle_nr: 0,
            backend_loop_wrapper: cxx::UniquePtr::null(),
        }
    }
}

impl crate::cxx_qt_lib_shoop::qquickitem::AsQQuickItem for LoopGui {
    unsafe fn mut_qquickitem_ptr(&mut self) -> *mut QQuickItem {
        qquickitem_from_ptr_loop(self as *mut Self)
    }
    unsafe fn ref_qquickitem_ptr(&self) -> *const QQuickItem {
        qquickitem_from_ref_loop(self) as *const QQuickItem
    }
}

impl IsQQuickItem for LoopGui {}

impl cxx_qt::Constructor<(*mut QQuickItem,)> for LoopGui {
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

    fn new(_parent: (*mut QQuickItem,)) -> LoopGuiRust {
        LoopGuiRust::default()
    }

    fn initialize(self: core::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        LoopGui::initialize_impl(self);
    }
}

impl cxx_qt::Constructor<()> for LoopGui {
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

    fn new(_args: ()) -> LoopGuiRust {
        LoopGuiRust::default()
    }

    fn initialize(self: core::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        LoopGui::initialize_impl(self);
    }
}
