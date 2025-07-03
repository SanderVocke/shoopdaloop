use common::logging::macros::*;
use backend_bindings::{Loop as BackendLoop, LoopState};

shoop_log_unit!("Frontend.Loop");

pub mod constants {
    pub const SIGNAL_CYCLED: &str = "cycled()";

    pub const INVOKABLE_UPDATE: &str = "update()";
    pub const INVOKABLE_TRANSITION: &str = "transition(::std::int32_t,::std::int32_t,::std::int32_t)";
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
        #[rust_name = "loop_backend_metatype_name"]
        unsafe fn meta_type_name(obj: &LoopBackend) -> Result<&str>;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qproperty(i32, mode, READ=get_mode, NOTIFY=mode_changed)]
        #[qproperty(i32, length, READ=get_length, NOTIFY=length_changed)]
        #[qproperty(i32, position, READ=get_position, NOTIFY=position_changed)]
        #[qproperty(i32, next_mode, READ=get_next_mode, NOTIFY=next_mode_changed)]
        #[qproperty(i32, next_transition_delay, READ=get_next_transition_delay, NOTIFY=next_transition_delay_changed)]
        // #[qproperty(QList_f32, display_peaks)]
        // #[qproperty(i32, display_midi_notes_active)]
        // #[qproperty(i32, display_midi_events_triggered)]
        // #[qproperty(QString, instance_identifier)]
        #[qproperty(i32, cycle_nr, READ=get_cycle_nr, NOTIFY=cycle_nr_changed)]
        #[qproperty(bool, initialized, READ=get_initialized, NOTIFY=initialized_changed)]
        #[qproperty(*mut QObject, backend, READ, NOTIFY=backend_changed)]
        #[qproperty(*mut QObject, sync_source, READ, NOTIFY=sync_source_changed)]
        #[qproperty(QString, instance_identifier, READ, NOTIFY=instance_identifier_changed)]
        type LoopBackend = super::LoopBackendRust;

        #[qinvokable]
        pub fn update(self: Pin<&mut LoopBackend>);

        #[qinvokable]
        pub fn set_length(self: Pin<&mut LoopBackend>, length: i32);

        #[qinvokable]
        pub fn set_position(self: Pin<&mut LoopBackend>, position: i32);

        #[qinvokable]
        pub fn set_backend(self: Pin<&mut LoopBackend>, backend: *mut QObject);

        #[qinvokable]
        pub unsafe fn set_sync_source(self: Pin<&mut LoopBackend>, sync_source: QVariant);

        #[qinvokable]
        pub fn set_instance_identifier(self: Pin<&mut LoopBackend>, instance_identifier: QString);

        #[qinvokable]
        pub fn transition_multiple(self: Pin<&mut LoopBackend>,
                                   loops: QList_QVariant,
                                   to_mode: i32,
                                   maybe_cycles_delay: i32,
                                   maybe_to_sync_at_cycle: i32);
        
        #[qinvokable]
        pub fn transition(self: Pin<&mut LoopBackend>,
                          to_mode: i32,
                          maybe_cycles_delay: i32,
                          maybe_to_sync_at_cycle: i32);

        #[qinvokable]
        pub fn clear(self: Pin<&mut LoopBackend>, length: i32);

        #[qinvokable]
        pub fn adopt_ringbuffers(self: Pin<&mut LoopBackend>,
                                 maybe_reverse_start_cycle : QVariant,
                                 maybe_cycles_length : QVariant,
                                 maybe_go_to_cycle : QVariant,
                                 go_to_mode : i32);

        #[qinvokable]
        pub fn get_mode(self: &LoopBackend) -> i32;

        #[qinvokable]
        pub fn get_length(self: &LoopBackend) -> i32;

        #[qinvokable]
        pub fn get_position(self: &LoopBackend) -> i32;

        #[qinvokable]
        pub fn get_next_mode(self: &LoopBackend) -> i32;

        #[qinvokable]
        pub fn get_next_transition_delay(self: &LoopBackend) -> i32;

        #[qinvokable]
        pub fn get_cycle_nr(self: &LoopBackend) -> i32;

        #[qinvokable]
        pub fn get_initialized(self: &LoopBackend) -> bool;

        #[qinvokable]
        pub fn maybe_initialize_backend(self: Pin<&mut LoopBackend>) -> bool;

        #[qsignal]
        fn cycled(self: Pin<&mut LoopBackend>, cycle_nr: i32);

        #[qsignal]
        fn starting_update(self: Pin<&mut LoopBackend>);

        #[qsignal]
        fn mode_changed(self: Pin<&mut LoopBackend>, new_mode : i32, old_mode : i32);

        #[qsignal]
        fn length_changed(self: Pin<&mut LoopBackend>, new_length : i32, old_length : i32);

        #[qsignal]
        fn position_changed(self: Pin<&mut LoopBackend>, new_position : i32, old_position : i32);

        #[qsignal]
        fn next_mode_changed(self: Pin<&mut LoopBackend>, new_next_mode : i32, old_next_mode : i32);

        #[qsignal]
        fn next_transition_delay_changed(self: Pin<&mut LoopBackend>, new_next_transition_delay : i32, old_next_transition_delay : i32);

        #[qsignal]
        fn cycle_nr_changed(self: Pin<&mut LoopBackend>, new_cycle_nr : i32, old_cycle_nr : i32);

        #[qsignal]
        unsafe fn sync_source_changed(self: Pin<&mut LoopBackend>, sync_source : *mut QObject);

        #[qsignal]
        fn instance_identifier_changed(self: Pin<&mut LoopBackend>, instance_identifier : QString);

        #[qsignal]
        fn initialized_changed(self: Pin<&mut LoopBackend>, initialized : bool);

        #[qsignal]
        unsafe fn backend_changed(self: Pin<&mut LoopBackend>, backend: *mut QObject);

        #[qsignal]
        fn state_changed(self: Pin<&mut LoopBackend>,
                         mode: i32,
                         length: i32,
                         position: i32,
                         next_mode: i32,
                         next_transition_delay: i32,
                         cycle_nr : i32);
    }

    unsafe extern "C++" {
        include!("cxx-qt-shoop/cast_ptr.h");
        #[rust_name = "qobject_to_loop_backend_ptr"]
        unsafe fn cast_qobject_ptr(obj : *mut QObject) -> *mut LoopBackend;

        include!("cxx-qt-shoop/qobject_classname.h");
        #[rust_name = "qobject_class_name_loop_backend"]
        fn qobject_class_name(obj: &LoopBackend) -> Result<&str>;

        include!("cxx-qt-lib-shoop/qobject.h");
        #[rust_name = "loop_backend_qobject_from_ptr"]
        unsafe fn qobjectFromPtr(obj : *mut LoopBackend) -> *mut QObject;

        #[rust_name = "loop_backend_qobject_from_ref"]
        fn qobjectFromRef(obj : &LoopBackend) -> &QObject;

        include!("cxx-qt-shoop/make_raw.h");
        #[rust_name = "make_raw_loop_backend"]
        unsafe fn make_raw() -> *mut LoopBackend;

    }

    impl cxx_qt::Constructor<()> for LoopBackend {}
}

pub use ffi::LoopBackend;
use ffi::*;
use crate::cxx_qt_lib_shoop::{qobject::AsQObject, qquickitem::IsQQuickItem};
use std::sync::{Arc, Mutex};

impl AsQObject for LoopBackend {
    unsafe fn mut_qobject_ptr(&mut self) -> *mut QObject {
        ffi::loop_backend_qobject_from_ptr(self as *mut Self)
    }

    unsafe fn ref_qobject_ptr(&self) -> *const QObject {
        ffi::loop_backend_qobject_from_ref(self) as *const QObject
    }
}

pub struct LoopBackendRust {
    // Properties
    pub sync_source : *mut QObject,
    pub backend: *mut QObject,
    pub instance_identifier: QString,

    // Rust members
    pub backend_loop : Option<BackendLoop>,
    pub prev_state : LoopState,
    pub prev_cycle_nr : i32,
}

impl Default for LoopBackendRust {
    fn default() -> LoopBackendRust {
        LoopBackendRust {
            backend: std::ptr::null_mut(),
            instance_identifier: QString::from("unknown"),
            backend_loop: None,
            prev_state: LoopState::default(),
            prev_cycle_nr: 0,
            sync_source: std::ptr::null_mut(),
        }
    }
}

impl cxx_qt::Constructor<()> for LoopBackend {
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

    fn new(_args: ()) -> LoopBackendRust {
        LoopBackendRust::default()
    }

    fn initialize(self: core::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        LoopBackend::initialize_impl(self);
    }
}