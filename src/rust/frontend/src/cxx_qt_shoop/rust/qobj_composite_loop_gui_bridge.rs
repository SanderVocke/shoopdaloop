#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");
        type QQuickItem = cxx_qt_lib_shoop::qquickitem::QQuickItem;
        type QObject = cxx_qt_lib_shoop::qobject::QObject;

        include!("cxx-qt-lib-shoop/qpointer.h");
        type QPointerQObject = cxx_qt_lib_shoop::qpointer::QPointerQObject;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qmap.h");
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;

        include!("cxx-qt-lib/qlist.h");
        type QList_QVariant = cxx_qt_lib::QList<cxx_qt_lib::QVariant>;

        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QQuickItem]
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
        #[qproperty(*mut QObject, backend, READ, WRITE=set_backend, NOTIFY=backend_changed)]
        #[qproperty(*mut QObject, sync_source, READ, WRITE=set_sync_source, NOTIFY=sync_source_changed)]
        #[qproperty(QMap_QString_QVariant, schedule, READ=get_schedule, WRITE=set_schedule, NOTIFY=schedule_changed)]
        #[qproperty(bool, play_after_record, READ, WRITE=set_play_after_record, NOTIFY=play_after_record_changed)]
        #[qproperty(bool, sync_mode_active, READ, WRITE=set_sync_mode_active, NOTIFY=sync_mode_active_changed)]
        #[qproperty(QString, kind, READ, WRITE=set_kind, NOTIFY=kind_changed)]
        #[qproperty(QString, instance_identifier, READ, WRITE=set_instance_identifier, NOTIFY=instance_identifier_changed)]
        // Other properties
        #[qproperty(*mut QObject, backend_loop_wrapper, READ=get_backend_loop_wrapper)]
        type CompositeLoopGui = super::CompositeLoopGuiRust;

        #[qinvokable]
        pub fn get_schedule(self: &CompositeLoopGui) -> QMap_QString_QVariant;

        #[qinvokable]
        pub fn transition(
            self: Pin<&mut CompositeLoopGui>,
            to_mode: i32,
            maybe_cycles_delay: i32,
            maybe_to_sync_at_cycle: i32,
        );

        #[qinvokable]
        pub fn adopt_ringbuffers(
            self: Pin<&mut CompositeLoopGui>,
            maybe_reverse_start_cycle: QVariant,
            maybe_cycles_length: QVariant,
            maybe_go_to_cycle: QVariant,
            go_to_mode: i32,
        );

        #[qinvokable]
        pub unsafe fn set_sync_source(self: Pin<&mut CompositeLoopGui>, sync_source: *mut QObject);

        #[qinvokable]
        pub unsafe fn set_backend(self: Pin<&mut CompositeLoopGui>, backend: *mut QObject);

        #[qinvokable]
        pub unsafe fn set_schedule(
            self: Pin<&mut CompositeLoopGui>,
            schedule: QMap_QString_QVariant,
        );

        #[qinvokable]
        pub unsafe fn set_play_after_record(
            self: Pin<&mut CompositeLoopGui>,
            play_after_record: bool,
        );

        #[qinvokable]
        pub unsafe fn set_sync_mode_active(
            self: Pin<&mut CompositeLoopGui>,
            sync_mode_active: bool,
        );

        #[qinvokable]
        pub unsafe fn set_kind(self: Pin<&mut CompositeLoopGui>, kind: QString);

        #[qinvokable]
        pub fn set_instance_identifier(
            self: Pin<&mut CompositeLoopGui>,
            instance_identifier: QString,
        );

        #[qinvokable]
        pub fn get_backend_loop_wrapper(self: Pin<&mut CompositeLoopGui>) -> *mut QObject;

        #[qinvokable]
        pub fn get_backend_loop_shared_ptr(self: Pin<&mut CompositeLoopGui>) -> QVariant;

        #[qsignal]
        #[cxx_name = "backendChanged"]
        pub unsafe fn backend_changed(self: Pin<&mut CompositeLoopGui>, backend: *mut QObject);

        #[qsignal]
        #[cxx_name = "syncSourceChanged"]
        pub unsafe fn sync_source_changed(
            self: Pin<&mut CompositeLoopGui>,
            sync_source: *mut QObject,
        );

        #[qsignal]
        #[cxx_name = "cycleNrChanged"]
        pub unsafe fn cycle_nr_changed(self: Pin<&mut CompositeLoopGui>, cycle_nr: i32);

        #[qsignal]
        #[cxx_name = "instanceIdentifierChanged"]
        pub unsafe fn instance_identifier_changed(
            self: Pin<&mut CompositeLoopGui>,
            instance_identifier: QString,
        );

        #[qsignal]
        #[cxx_name = "initializedChanged"]
        pub unsafe fn initialized_changed(self: Pin<&mut CompositeLoopGui>, initialized: bool);

        #[qsignal]
        #[cxx_name = "nextModeChanged"]
        pub unsafe fn next_mode_changed(self: Pin<&mut CompositeLoopGui>);

        #[qsignal]
        #[cxx_name = "modeChanged"]
        pub unsafe fn mode_changed(self: Pin<&mut CompositeLoopGui>);

        #[qsignal]
        #[cxx_name = "lengthChanged"]
        pub unsafe fn length_changed(self: Pin<&mut CompositeLoopGui>);

        #[qsignal]
        #[cxx_name = "positionChanged"]
        pub unsafe fn position_changed(self: Pin<&mut CompositeLoopGui>);

        #[qsignal]
        #[cxx_name = "nextTransitionDelayChanged"]
        pub unsafe fn next_transition_delay_changed(self: Pin<&mut CompositeLoopGui>);

        #[qsignal]
        #[cxx_name = "iterationChanged"]
        pub unsafe fn iteration_changed(self: Pin<&mut CompositeLoopGui>, iteration: i32);

        #[qsignal]
        #[cxx_name = "nCyclesChanged"]
        pub unsafe fn n_cycles_changed(self: Pin<&mut CompositeLoopGui>);

        #[qsignal]
        #[cxx_name = "syncPositionChanged"]
        pub unsafe fn sync_position_changed(self: Pin<&mut CompositeLoopGui>);

        #[qsignal]
        #[cxx_name = "syncLengthChanged"]
        pub unsafe fn sync_length_changed(self: Pin<&mut CompositeLoopGui>);

        #[qsignal]
        #[cxx_name = "runningLoopsChanged"]
        pub unsafe fn running_loops_changed(self: Pin<&mut CompositeLoopGui>);

        #[qsignal]
        #[cxx_name = "scheduleChanged"]
        pub unsafe fn schedule_changed(
            self: Pin<&mut CompositeLoopGui>,
            schedule: QMap_QString_QVariant,
        );

        #[qsignal]
        #[cxx_name = "playAfterRecordChanged"]
        pub unsafe fn play_after_record_changed(self: Pin<&mut CompositeLoopGui>);

        #[qsignal]
        #[cxx_name = "syncModeActiveChanged"]
        pub unsafe fn sync_mode_active_changed(self: Pin<&mut CompositeLoopGui>);

        #[qsignal]
        #[cxx_name = "kindChanged"]
        pub unsafe fn kind_changed(self: Pin<&mut CompositeLoopGui>);

        #[qsignal]
        fn cycled(self: Pin<&mut CompositeLoopGui>, cycle_nr: i32);

        // The following signals are to internally connect to the back-end object
        // which lives on another thread.
        #[qsignal]
        fn backend_set_instance_identifier(
            self: Pin<&mut CompositeLoopGui>,
            instance_identifier: QString,
        );
        #[qsignal]
        fn backend_clear(self: Pin<&mut CompositeLoopGui>);
        #[qsignal]
        fn backend_set_kind(self: Pin<&mut CompositeLoopGui>, kind: QString);
        #[qsignal]
        fn backend_set_schedule(self: Pin<&mut CompositeLoopGui>, schedule: QMap_QString_QVariant);
        #[qsignal]
        fn backend_set_play_after_record(self: Pin<&mut CompositeLoopGui>, play_after_record: bool);
        #[qsignal]
        fn backend_set_sync_mode_active(self: Pin<&mut CompositeLoopGui>, sync_mode_active: bool);
        #[qsignal]
        fn backend_transition(
            self: Pin<&mut CompositeLoopGui>,
            to_mode: i32,
            maybe_cycles_delay: i32,
            maybe_to_sync_at_cycle: i32,
        );
        #[qsignal]
        fn backend_adopt_ringbuffers(
            self: Pin<&mut CompositeLoopGui>,
            maybe_reverse_start_cycle: QVariant,
            maybe_cycles_length: QVariant,
            maybe_go_to_cycle: QVariant,
            go_to_mode: i32,
        );
        #[qsignal]
        pub unsafe fn backend_set_sync_source(
            self: Pin<&mut CompositeLoopGui>,
            sync_source: *mut QObject,
        );

        #[qsignal]
        pub fn backend_transition_multiple(
            self: Pin<&mut CompositeLoopGui>,
            loops: QList_QVariant,
            to_mode: i32,
            maybe_cycles_delay: i32,
            maybe_to_sync_at_cycle: i32,
        );

        #[qinvokable]
        pub fn on_backend_n_cycles_changed(self: Pin<&mut CompositeLoopGui>, n_cycles: i32);

        #[qinvokable]
        pub fn on_backend_sync_length_changed(self: Pin<&mut CompositeLoopGui>, sync_length: i32);

        #[qinvokable]
        pub fn on_backend_iteration_changed(self: Pin<&mut CompositeLoopGui>, iteration: i32);

        #[qinvokable]
        pub fn on_backend_mode_changed(self: Pin<&mut CompositeLoopGui>, mode: i32);

        #[qinvokable]
        pub fn on_backend_sync_position_changed(
            self: Pin<&mut CompositeLoopGui>,
            sync_position: i32,
        );

        #[qinvokable]
        pub fn on_backend_next_mode_changed(self: Pin<&mut CompositeLoopGui>, next_mode: i32);

        #[qinvokable]
        pub fn on_backend_next_transition_delay_changed(
            self: Pin<&mut CompositeLoopGui>,
            next_transition_delay: i32,
        );

        #[qinvokable]
        pub fn on_backend_running_loops_changed(
            self: Pin<&mut CompositeLoopGui>,
            running_loops: QList_QVariant,
        );

        #[qinvokable]
        pub fn on_backend_length_changed(self: Pin<&mut CompositeLoopGui>, length: i32);

        #[qinvokable]
        pub fn on_backend_position_changed(self: Pin<&mut CompositeLoopGui>, position: i32);

        #[qinvokable]
        pub fn on_backend_cycled(self: Pin<&mut CompositeLoopGui>, cycle_nr: i32);

        #[qinvokable]
        pub fn on_backend_initialized_changed(self: Pin<&mut CompositeLoopGui>, initialized: bool);

        #[qinvokable]
        pub fn transition_multiple(
            self: Pin<&mut CompositeLoopGui>,
            loops: QList_QVariant,
            to_mode: i32,
            maybe_cycles_delay: i32,
            maybe_to_sync_at_cycle: i32,
        );
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");

        #[rust_name = "qquickitem_from_ref_composite_loop"]
        unsafe fn qquickitemFromRef(obj: &CompositeLoopGui) -> &QQuickItem;

        #[rust_name = "qquickitem_from_ptr_composite_loop"]
        unsafe fn qquickitemFromPtr(obj: *mut CompositeLoopGui) -> *mut QQuickItem;

        include!("cxx-qt-lib-shoop/register_qml_type.h");
        #[rust_name = "register_qml_type_composite_loop_gui"]
        unsafe fn register_qml_type(
            inference_example: *mut CompositeLoopGui,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );

        include!("cxx-qt-lib-shoop/make_unique.h");
        #[rust_name = "make_unique_composite_loop_gui"]
        fn make_unique() -> UniquePtr<CompositeLoopGui>;

        include!("cxx-qt-lib-shoop/cast_ptr.h");
        #[rust_name = "qobject_to_composite_loop_gui_ptr"]
        unsafe fn cast_qobject_ptr(obj: *mut QObject) -> *mut CompositeLoopGui;
    }

    impl cxx_qt::Constructor<(*mut QQuickItem,), NewArguments = (*mut QQuickItem,)>
        for CompositeLoopGui
    {
    }
    impl cxx_qt::Constructor<()> for CompositeLoopGui {}
}

use backend_bindings::LoopMode;
use cxx;
use cxx_qt_lib_shoop::qsharedpointer_qobject::QSharedPointer_QObject;
pub use ffi::CompositeLoopGui;
use ffi::*;

impl cxx_qt_lib_shoop::qquickitem::AsQQuickItem for CompositeLoopGui {
    unsafe fn mut_qquickitem_ptr(&mut self) -> *mut QQuickItem {
        qquickitem_from_ptr_composite_loop(self as *mut Self)
    }
    unsafe fn ref_qquickitem_ptr(&self) -> *const QQuickItem {
        qquickitem_from_ref_composite_loop(self) as *const QQuickItem
    }
}

use cxx_qt_lib_shoop::qquickitem::IsQQuickItem;

use crate::composite_loop_schedule::CompositeLoopSchedule;
impl IsQQuickItem for CompositeLoopGui {}

impl cxx_qt::Constructor<(*mut QQuickItem,)> for CompositeLoopGui {
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

    fn new(_parent: (*mut QQuickItem,)) -> CompositeLoopGuiRust {
        CompositeLoopGuiRust::default()
    }

    fn initialize(self: core::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        CompositeLoopGui::initialize_impl(self);
    }
}

impl cxx_qt::Constructor<()> for CompositeLoopGui {
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

    fn new(_args: ()) -> CompositeLoopGuiRust {
        CompositeLoopGuiRust::default()
    }

    fn initialize(self: core::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        CompositeLoopGui::initialize_impl(self);
    }
}

pub struct CompositeLoopGuiRust {
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
    pub backend_loop_wrapper: cxx::UniquePtr<QSharedPointer_QObject>,
    pub cycle_nr: i32,

    // Others
    pub schedule: CompositeLoopSchedule,
}

impl Default for CompositeLoopGuiRust {
    fn default() -> CompositeLoopGuiRust {
        CompositeLoopGuiRust {
            running_loops: QList_QVariant::default(),
            initialized: false,
            iteration: -1,
            mode: LoopMode::Stopped as isize as i32,
            next_mode: -1,
            next_transition_delay: -1,
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
            instance_identifier: QString::from("unknown"),
            backend_loop_wrapper: cxx::UniquePtr::null(),
            cycle_nr: 0,
            schedule: CompositeLoopSchedule::default(),
        }
    }
}
