#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");
        type QQuickItem = cxx_qt_lib_shoop::qquickitem::QQuickItem;
        type QObject = cxx_qt_lib_shoop::qobject::QObject;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qmap.h");
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;

        include!("cxx-qt-lib/qlist.h");
        type QList_QVariant = cxx_qt_lib::QList<cxx_qt_lib::QVariant>;
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
        // Frontend -> Backend properties
        #[qproperty(*mut QObject, backend, READ, WRITE=set_backend, NOTIFY=backend_changed)]
        #[qproperty(*mut QObject, sync_source, READ, WRITE=set_sync_source, NOTIFY=sync_source_changed)]
        #[qproperty(QMap_QString_QVariant, schedule, READ, WRITE=set_schedule, NOTIFY=schedule_changed)]
        #[qproperty(bool, play_after_record, READ, WRITE=set_play_after_record, NOTIFY=play_after_record_changed)]
        #[qproperty(bool, sync_mode_active, READ, WRITE=set_sync_mode_active, NOTIFY=sync_mode_active_changed)]
        #[qproperty(str, kind, READ, WRITE=set_kind, NOTIFY=kind_changed)]
        #[qproperty(QString, instance_identifier, READ, WRITE=set_instance_identifier, NOTIFY=instance_identifier_changed)]
        // Other properties
        #[qproperty(*mut QObject, backend_loop_wrapper, READ=get_backend_loop_wrapper)]
        type CompositeLoopGui = super::CompositeLoopGuiRust;

        #[qinvokable]
        pub fn transition(self: Pin<&mut CompositeLoopGui>, to_mode: i32, maybe_cycles_delay: i32, maybe_to_sync_at_cycle: i32);

        #[qinvokable]
        pub fn adopt_ringbuffers(
            self: Pin<&mut LoopGui>,
            maybe_reverse_start_cycle: QVariant,
            maybe_cycles_length: QVariant,
            maybe_go_to_cycle: QVariant,
            go_to_mode: i32,
        );

        #[qinvokable]
        pub unsafe fn set_sync_source(self: Pin<&mut LoopGui>, sync_source: *mut QObject);

        #[qinvokable]
        pub unsafe fn set_backend(self: Pin<&mut LoopGui>, backend: *mut QObject);

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
        fn cycled(self: Pin<&mut LoopGui>, cycle_nr: i32);
    }

    unsafe extern "C++" {
    }
}

pub use ffi::CompositeLoopGui;

pub struct CompositeLoopGuiRust {}

impl Default for CompositeLoopGuiRust {
    fn default() -> CompositeLoopGuiRust {
        CompositeLoopGuiRust {}
    }
}
