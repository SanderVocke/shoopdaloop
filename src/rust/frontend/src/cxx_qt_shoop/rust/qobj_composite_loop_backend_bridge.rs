use cxx_qt_lib_shoop::qobject::AsQObject;
use std::pin::Pin;

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qobject.h");
        type QObject = cxx_qt_lib_shoop::qobject::QObject;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qmap.h");
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;

        include!("cxx-qt-lib/qlist.h");
        type QList_QVariant = cxx_qt_lib::QList<cxx_qt_lib::QVariant>;
    }

    unsafe extern "RustQt" {
        #[qobject]
        // Backend -> Frontend properties
        #[qproperty(QList_QVariant, running_loops, READ, NOTIFY=running_loops_changed)]
        #[qproperty(bool, initialized, READ, NOTIFY=initialized_changed)]
        #[qproperty(i32, iteration, READ, NOTIFY=iteration_changed)]
        #[qproperty(i32, mode, READ, NOTIFY=mode_changed)]
        #[qproperty(i32, next_mode, READ, NOTIFY=next_mode_changed)]
        #[qproperty(i32, next_transition_delay, READ, NOTIFY=next_transition_delay_changed)]
        #[qproperty(i32, n_cycles, READ, NOTIFY=n_cycles_changed)]
        #[qproperty(i32, length, READ, NOTIFY=length_changed)]
        #[qproperty(i32, sync_position, READ, NOTIFY=sync_position_changed)]
        #[qproperty(i32, sync_length, READ, NOTIFY=sync_length_changed)]
        #[qproperty(i32, position, READ, NOTIFY=position_changed)]
        #[qproperty(i32, cycle_nr, READ, NOTIFY=cycle_nr_changed)]
        // Frontend -> Backend properties
        #[qproperty(*mut QObject, backend, READ, NOTIFY=backend_changed)]
        #[qproperty(*mut QObject, sync_source, READ, NOTIFY=sync_source_changed)]
        #[qproperty(QMap_QString_QVariant, schedule, READ=get_schedule, NOTIFY=schedule_changed)]
        #[qproperty(bool, play_after_record, READ, NOTIFY=play_after_record_changed)]
        #[qproperty(bool, sync_mode_active, READ, NOTIFY=sync_mode_active_changed)]
        #[qproperty(QString, kind, READ, WRITE=set_kind, NOTIFY=kind_changed)]
        #[qproperty(QString, instance_identifier, READ, NOTIFY=instance_identifier_changed)]
        // Other properties
        #[qproperty(*mut QObject, frontend_loop, READ, WRITE)]
        type CompositeLoopBackend = super::CompositeLoopBackendRust;

        #[qinvokable]
        pub fn get_schedule(self: &CompositeLoopBackend) -> QMap_QString_QVariant;

        #[qinvokable]
        pub fn transition(
            self: Pin<&mut CompositeLoopBackend>,
            to_mode: i32,
            maybe_cycles_delay: i32,
            maybe_to_sync_at_cycle: i32,
        );

        #[qinvokable]
        pub fn adopt_ringbuffers(
            self: Pin<&mut CompositeLoopBackend>,
            maybe_reverse_start_cycle: QVariant,
            maybe_cycles_length: QVariant,
            maybe_go_to_cycle: QVariant,
            go_to_mode: i32,
        );

        #[qinvokable]
        pub unsafe fn set_sync_source(
            self: Pin<&mut CompositeLoopBackend>,
            sync_source: *mut QObject,
        );

        #[qinvokable]
        pub unsafe fn set_backend(self: Pin<&mut CompositeLoopBackend>, backend: *mut QObject);

        #[qinvokable]
        pub unsafe fn set_schedule(
            self: Pin<&mut CompositeLoopBackend>,
            schedule: QMap_QString_QVariant,
        );

        #[qinvokable]
        pub fn clear(self: Pin<&mut CompositeLoopBackend>);

        #[qinvokable]
        pub unsafe fn set_play_after_record(
            self: Pin<&mut CompositeLoopBackend>,
            play_after_record: bool,
        );

        #[qinvokable]
        pub unsafe fn set_sync_mode_active(
            self: Pin<&mut CompositeLoopBackend>,
            sync_mode_active: bool,
        );

        #[qinvokable]
        pub unsafe fn set_kind(self: Pin<&mut CompositeLoopBackend>, kind: QString);

        #[qinvokable]
        pub fn set_instance_identifier(
            self: Pin<&mut CompositeLoopBackend>,
            instance_identifier: QString,
        );

        #[qinvokable]
        pub fn update(self: Pin<&mut CompositeLoopBackend>);

        #[qinvokable]
        pub fn update_sync_position(self: Pin<&mut CompositeLoopBackend>);

        #[qinvokable]
        pub fn update_sync_length(self: Pin<&mut CompositeLoopBackend>);

        #[qinvokable]
        pub fn handle_sync_loop_trigger(self: Pin<&mut CompositeLoopBackend>, cycle_nr: i32);

        #[qsignal]
        #[cxx_name = "stateChanged"]
        fn state_changed(
            self: Pin<&mut CompositeLoopBackend>,
            mode: i32,
            length: i32,
            position: i32,
            next_mode: i32,
            next_transition_delay: i32,
            cycle_nr: i32,
            sync_position: i32,
            sync_length: i32,
            iteration: i32,
            running_loops: QList_QVariant,
        );

        #[qsignal]
        #[cxx_name = "scheduleChanged"]
        pub unsafe fn schedule_changed(
            self: Pin<&mut CompositeLoopBackend>,
            schedule: QMap_QString_QVariant,
        );

        #[qsignal]
        #[cxx_name = "cycleNrChanged"]
        pub unsafe fn cycle_nr_changed(self: Pin<&mut CompositeLoopBackend>, cycle_nr: i32);

        #[qsignal]
        #[cxx_name = "backendChanged"]
        pub unsafe fn backend_changed(self: Pin<&mut CompositeLoopBackend>, backend: *mut QObject);

        #[qsignal]
        #[cxx_name = "syncSourceChanged"]
        pub unsafe fn sync_source_changed(
            self: Pin<&mut CompositeLoopBackend>,
            sync_source: *mut QObject,
        );

        #[qsignal]
        #[cxx_name = "instanceIdentifierChanged"]
        pub unsafe fn instance_identifier_changed(
            self: Pin<&mut CompositeLoopBackend>,
            instance_identifier: QString,
        );

        #[qsignal]
        #[cxx_name = "initializedChanged"]
        pub unsafe fn initialized_changed(self: Pin<&mut CompositeLoopBackend>, initialized: bool);

        #[qsignal]
        #[cxx_name = "iterationChanged"]
        pub unsafe fn iteration_changed(self: Pin<&mut CompositeLoopBackend>, iteration: i32);

        #[qsignal]
        #[cxx_name = "nextModeChanged"]
        pub unsafe fn next_mode_changed(self: Pin<&mut CompositeLoopBackend>);

        #[qsignal]
        #[cxx_name = "modeChanged"]
        pub unsafe fn mode_changed(self: Pin<&mut CompositeLoopBackend>);

        #[qsignal]
        #[cxx_name = "lengthChanged"]
        pub unsafe fn length_changed(self: Pin<&mut CompositeLoopBackend>);

        #[qsignal]
        #[cxx_name = "positionChanged"]
        pub unsafe fn position_changed(self: Pin<&mut CompositeLoopBackend>);

        #[qsignal]
        #[cxx_name = "nextTransitionDelayChanged"]
        pub unsafe fn next_transition_delay_changed(self: Pin<&mut CompositeLoopBackend>);

        #[qsignal]
        #[cxx_name = "nCyclesChanged"]
        pub unsafe fn n_cycles_changed(self: Pin<&mut CompositeLoopBackend>);

        #[qsignal]
        #[cxx_name = "syncPositionChanged"]
        pub unsafe fn sync_position_changed(self: Pin<&mut CompositeLoopBackend>);

        #[qsignal]
        #[cxx_name = "syncLengthChanged"]
        pub unsafe fn sync_length_changed(self: Pin<&mut CompositeLoopBackend>);

        #[qsignal]
        #[cxx_name = "runningLoopsChanged"]
        pub unsafe fn running_loops_changed(self: Pin<&mut CompositeLoopBackend>);

        #[qsignal]
        #[cxx_name = "playAfterRecordChanged"]
        pub unsafe fn play_after_record_changed(self: Pin<&mut CompositeLoopBackend>);

        #[qsignal]
        #[cxx_name = "syncModeActiveChanged"]
        pub unsafe fn sync_mode_active_changed(self: Pin<&mut CompositeLoopBackend>);

        #[qsignal]
        #[cxx_name = "kindChanged"]
        pub unsafe fn kind_changed(self: Pin<&mut CompositeLoopBackend>);

        #[qsignal]
        fn cycled(self: Pin<&mut CompositeLoopBackend>, cycle_nr: i32);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/make_raw.h");
        #[rust_name = "make_raw_composite_loop_backend"]
        unsafe fn make_raw() -> *mut CompositeLoopBackend;

        include!("cxx-qt-lib-shoop/qobject.h");
        #[rust_name = "composite_loop_backend_qobject_from_ptr"]
        unsafe fn qobjectFromPtr(obj: *mut CompositeLoopBackend) -> *mut QObject;

        #[rust_name = "composite_loop_backend_qobject_from_ref"]
        fn qobjectFromRef(obj: &CompositeLoopBackend) -> &QObject;
    }
}

pub use ffi::make_raw_composite_loop_backend;
pub use ffi::CompositeLoopBackend;
use ffi::*;

use crate::composite_loop_schedule::CompositeLoopSchedule;

impl AsQObject for CompositeLoopBackend {
    unsafe fn mut_qobject_ptr(&mut self) -> *mut ffi::QObject {
        ffi::composite_loop_backend_qobject_from_ptr(self as *mut Self)
    }

    unsafe fn ref_qobject_ptr(&self) -> *const ffi::QObject {
        ffi::composite_loop_backend_qobject_from_ref(self) as *const ffi::QObject
    }
}

impl cxx_qt::Constructor<()> for CompositeLoopBackend {
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

    fn new(_args: ()) -> CompositeLoopBackendRust {
        CompositeLoopBackendRust::default()
    }

    fn initialize(self: core::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        CompositeLoopBackend::initialize_impl(self);
    }
}

pub struct CompositeLoopBackendRust {
    // Properties
    pub running_loops: QList_QVariant,
    pub initialized: bool,
    pub iteration: i32,
    pub mode: i32,
    pub next_mode: i32,
    pub next_transition_delay: i32,
    pub n_cycles: i32,
    pub length: i32,
    pub sync_position: i32,
    pub sync_length: i32,
    pub position: i32,
    pub backend: *mut QObject,
    pub sync_source: *mut QObject,
    pub play_after_record: bool,
    pub sync_mode_active: bool,
    pub kind: QString,
    pub instance_identifier: QString,
    pub cycle_nr: i32,
    pub frontend_loop: *mut QObject,

    // Others
    pub schedule: CompositeLoopSchedule,
}

impl Default for CompositeLoopBackendRust {
    fn default() -> CompositeLoopBackendRust {
        CompositeLoopBackendRust {
            running_loops: QList_QVariant::default(),
            initialized: false,
            iteration: 0,
            mode: 0,
            next_mode: 0,
            next_transition_delay: 0,
            n_cycles: 0,
            length: 0,
            sync_position: 0,
            sync_length: 0,
            position: 0,
            backend: std::ptr::null_mut(),
            sync_source: std::ptr::null_mut(),
            play_after_record: false,
            sync_mode_active: false,
            kind: QString::default(),
            instance_identifier: QString::default(),
            cycle_nr: 0,
            frontend_loop: std::ptr::null_mut(),
            schedule: CompositeLoopSchedule::default(),
        }
    }
}
