use common::logging::macros::*;
use cxx_qt_lib_shoop::qpointer::QPointerQObject;
use shoop_engine::app_backend::Loop as BackendLoop;
use shoop_engine::LoopState;

shoop_log_unit!("Frontend.Loop");

#[cxx_qt::bridge]
pub mod ffi {
    extern "C++" {
        #[doc(hidden)]
        #[namespace = ""]
        type QObject = cxx_qt::QObject;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");
        type QQuickItem = cxx_qt_lib_shoop::qquickitem::QQuickItem;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qlist.h");
        type QList_QVariant = cxx_qt_lib::QList<cxx_qt_lib::QVariant>;
        type QList_f32 = cxx_qt_lib::QList<f32>;

        include!("cxx-qt-lib/qmap.h");
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;

        include!("cxx-qt-lib-shoop/qmetatype.h");
        #[rust_name = "loop_gui_metatype_name"]
        unsafe fn meta_type_name(obj: *mut LoopGui) -> Result<String>;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QQuickItem]
        #[qproperty(i32, mode, READ=get_mode, NOTIFY=mode_changed)]
        #[qproperty(i32, length, READ=get_length, NOTIFY=length_changed)]
        #[qproperty(i32, position, READ=get_position, NOTIFY=position_changed)]
        #[qproperty(i32, next_mode, READ=get_next_mode, NOTIFY=next_mode_changed)]
        #[qproperty(i32, next_transition_delay, READ=get_next_transition_delay, NOTIFY=next_transition_delay_changed)]
        #[qproperty(i32, cycle_nr, READ=get_cycle_nr, NOTIFY=cycle_nr_changed)]
        #[qproperty(bool, initialized, READ=get_initialized, NOTIFY=initialized_changed)]
        #[qproperty(*mut QObject, backend, READ, WRITE=set_backend, NOTIFY=backend_changed)]
        #[qproperty(*mut QObject, sync_source, READ, WRITE=set_sync_source, NOTIFY=sync_source_changed)]
        #[qproperty(QString, instance_identifier, READ, WRITE=set_instance_identifier, NOTIFY=instance_identifier_changed)]
        type LoopGui = super::LoopGuiRust;

        #[qinvokable]
        pub fn update(self: Pin<&mut LoopGui>);

        #[qinvokable]
        pub fn set_length(self: Pin<&mut LoopGui>, length: i32);

        #[qinvokable]
        pub fn set_position(self: Pin<&mut LoopGui>, position: i32);

        #[qinvokable]
        pub fn set_backend(self: Pin<&mut LoopGui>, backend: *mut QObject);

        #[qinvokable]
        pub unsafe fn set_sync_source(self: Pin<&mut LoopGui>, sync_source: *mut QObject);

        #[qinvokable]
        pub fn set_instance_identifier(self: Pin<&mut LoopGui>, instance_identifier: QString);

        // For any backend loop, will split into unison/individual
        // transitions
        #[qinvokable]
        pub fn transition_multiple(
            self: Pin<&mut LoopGui>,
            loops: QList_QVariant,
            to_mode: i32,
            maybe_cycles_delay: i32,
            maybe_to_sync_at_cycle: i32,
        );

        // For LoopGui objects only
        #[qinvokable]
        pub fn transition_multiple_backend_in_unison(
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
        pub fn adopt_ringbuffers(
            self: Pin<&mut LoopGui>,
            maybe_reverse_start_cycle: QVariant,
            maybe_cycles_length: QVariant,
            maybe_go_to_cycle: QVariant,
            go_to_mode: i32,
        );

        #[qinvokable]
        pub fn get_mode(self: &LoopGui) -> i32;

        #[qinvokable]
        pub fn get_length(self: &LoopGui) -> i32;

        #[qinvokable]
        pub fn get_position(self: &LoopGui) -> i32;

        #[qinvokable]
        pub fn get_next_mode(self: &LoopGui) -> i32;

        #[qinvokable]
        pub fn get_next_transition_delay(self: &LoopGui) -> i32;

        #[qinvokable]
        pub fn get_cycle_nr(self: &LoopGui) -> i32;

        #[qinvokable]
        pub fn get_initialized(self: &LoopGui) -> bool;

        #[qinvokable]
        pub fn maybe_initialize_backend(self: Pin<&mut LoopGui>) -> bool;

        #[qinvokable]
        pub fn queue_set_length(self: Pin<&mut LoopGui>, length: i32);

        #[qinvokable]
        pub fn queue_set_position(self: Pin<&mut LoopGui>, position: i32);

        #[qinvokable]
        pub fn deinit(self: Pin<&mut LoopGui>);

        #[qsignal]
        fn cycled(self: Pin<&mut LoopGui>, cycle_nr: i32);

        #[qsignal]
        fn starting_update(self: Pin<&mut LoopGui>);

        #[qsignal]
        #[cxx_name = "modeChanged"]
        fn mode_changed(self: Pin<&mut LoopGui>, new_mode: i32, old_mode: i32);

        #[qsignal]
        #[cxx_name = "lengthChanged"]
        fn length_changed(self: Pin<&mut LoopGui>, new_length: i32, old_length: i32);

        #[qsignal]
        #[cxx_name = "positionChanged"]
        fn position_changed(self: Pin<&mut LoopGui>, new_position: i32, old_position: i32);

        #[qsignal]
        #[cxx_name = "nextModeChanged"]
        fn next_mode_changed(self: Pin<&mut LoopGui>, new_next_mode: i32, old_next_mode: i32);

        #[qsignal]
        #[cxx_name = "nextTransitionDelayChanged"]
        fn next_transition_delay_changed(
            self: Pin<&mut LoopGui>,
            new_next_transition_delay: i32,
            old_next_transition_delay: i32,
        );

        #[qsignal]
        #[cxx_name = "cycleNrChanged"]
        fn cycle_nr_changed(self: Pin<&mut LoopGui>, new_cycle_nr: i32, old_cycle_nr: i32);

        #[qsignal]
        #[cxx_name = "syncSourceChanged"]
        unsafe fn sync_source_changed(self: Pin<&mut LoopGui>, sync_source: *mut QObject);

        #[qsignal]
        #[cxx_name = "instanceIdentifierChanged"]
        fn instance_identifier_changed(self: Pin<&mut LoopGui>, instance_identifier: QString);

        #[qsignal]
        #[cxx_name = "initializedChanged"]
        fn initialized_changed(self: Pin<&mut LoopGui>, initialized: bool);

        #[qsignal]
        #[cxx_name = "backendChanged"]
        unsafe fn backend_changed(self: Pin<&mut LoopGui>, backend: *mut QObject);

        #[qsignal]
        #[cxx_name = "stateChanged"]
        fn state_changed(
            self: Pin<&mut LoopGui>,
            mode: i32,
            length: i32,
            position: i32,
            next_mode: i32,
            next_transition_delay: i32,
            cycle_nr: i32,
        );
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qobject.h");
        include!("cxx-qt-lib-shoop/qquickitem.h");
        include!("cxx-qt-lib-shoop/cast_ptr.h");

        #[rust_name = "qquickitem_from_ref_loop_gui"]
        unsafe fn qquickitemFromRef(obj: &LoopGui) -> &QQuickItem;

        #[rust_name = "qquickitem_from_ptr_loop_gui"]
        unsafe fn qquickitemFromPtr(obj: *mut LoopGui) -> *mut QQuickItem;
        #[rust_name = "qobject_to_loop_gui_ptr"]
        unsafe fn cast_qobject_ptr(obj: *mut QObject) -> *mut LoopGui;

        include!("cxx-qt-lib-shoop/qobject_classname.h");
        #[rust_name = "qobject_class_name_loop_gui"]
        fn qobject_class_name(obj: &LoopGui) -> Result<&str>;

        #[rust_name = "loop_gui_qobject_from_ptr"]
        unsafe fn qobjectFromPtr(obj: *mut LoopGui) -> *mut QObject;

        #[rust_name = "loop_gui_qobject_from_ref"]
        fn qobjectFromRef(obj: &LoopGui) -> &QObject;

        #[rust_name = "from_qobject_ref_loop_gui"]
        unsafe fn fromQObjectRef(obj: &QObject, output: *mut *const LoopGui);

        #[rust_name = "from_qobject_mut_loop_gui"]
        unsafe fn fromQObjectMut(obj: Pin<&mut QObject>, output: *mut *mut LoopGui);

        include!("cxx-qt-lib-shoop/register_qml_type.h");
        #[rust_name = "register_qml_type_loop_gui"]
        unsafe fn register_qml_type(
            inference_example: *mut LoopGui,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );
    }

    impl cxx_qt::Constructor<(*mut QQuickItem,), NewArguments = (*mut QQuickItem,)> for LoopGui {}
    impl cxx_qt::Constructor<()> for LoopGui {}
}

pub use ffi::LoopGui;
use ffi::*;

impl cxx_qt_lib_shoop::qobject::FromQObject for LoopGui {
    unsafe fn ptr_from_qobject_ref(obj: &cxx_qt::QObject) -> *const Self {
        let mut output: *const Self = std::ptr::null();
        from_qobject_ref_loop_gui(obj, &mut output as *mut *const Self);
        output
    }

    unsafe fn ptr_from_qobject_mut(obj: std::pin::Pin<&mut cxx_qt::QObject>) -> *mut Self {
        let mut output: *mut Self = std::ptr::null_mut();
        from_qobject_mut_loop_gui(obj, &mut output as *mut *mut Self);
        output
    }
}

pub struct LoopGuiRust {
    // Properties
    pub sync_source: *mut QObject,
    pub backend: *mut QObject,
    pub instance_identifier: QString,

    // Rust members
    pub backend_loop: Option<BackendLoop>,
    pub sync_source_guard: cxx::UniquePtr<QPointerQObject>,
    pub sync_source_applied_session_id: Option<u64>,
    pub prev_state: LoopState,
    pub prev_cycle_nr: i32,
}

impl Default for LoopGuiRust {
    fn default() -> LoopGuiRust {
        LoopGuiRust {
            backend: std::ptr::null_mut(),
            instance_identifier: QString::from("unknown"),
            backend_loop: None,
            sync_source_guard: cxx::UniquePtr::null(),
            sync_source_applied_session_id: None,
            prev_state: LoopState {
                mode: shoop_engine::LoopMode::Stopped,
                ..LoopState::default()
            },
            prev_cycle_nr: 0,
            sync_source: std::ptr::null_mut(),
        }
    }
}

impl cxx_qt_lib_shoop::qquickitem::AsQQuickItem for LoopGui {
    unsafe fn mut_qquickitem_ptr(&mut self) -> *mut QQuickItem {
        qquickitem_from_ptr_loop_gui(self as *mut Self)
    }

    unsafe fn ref_qquickitem_ptr(&self) -> *const QQuickItem {
        qquickitem_from_ref_loop_gui(self) as *const QQuickItem
    }
}

impl cxx_qt_lib_shoop::qquickitem::IsQQuickItem for LoopGui {}

impl cxx_qt::Constructor<(*mut QQuickItem,)> for LoopGui {
    type BaseArguments = (*mut QQuickItem,);
    type InitializeArguments = ();
    type NewArguments = (*mut QQuickItem,);

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
    type BaseArguments = ();
    type InitializeArguments = ();
    type NewArguments = ();

    fn route_arguments(
        args: (),
    ) -> (
        Self::NewArguments,
        Self::BaseArguments,
        Self::InitializeArguments,
    ) {
        (args, args, ())
    }

    fn new(_args: ()) -> LoopGuiRust {
        LoopGuiRust::default()
    }

    fn initialize(self: core::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        LoopGui::initialize_impl(self);
    }
}
