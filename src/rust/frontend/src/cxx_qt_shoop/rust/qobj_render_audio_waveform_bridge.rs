use common::logging::macros::*;
shoop_log_unit!("Frontend.RenderAudioWaveform");

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qcolor.h");
        type QColor = cxx_qt_lib::QColor;

        include!("cxx-qt-lib/qrectf.h");
        type QRectF = cxx_qt_lib::QRectF;

        include!("cxx-qt-lib/qsizef.h");
        type QSizeF = cxx_qt_lib::QSizeF;

        include!("cxx-qt-lib/qlist.h");
        type QList_f64 = cxx_qt_lib::QList<f64>;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qlinef.h");
        type QLineF = cxx_qt_lib::QLineF;

        include!("cxx-qt-lib/qpen.h");
        type QPen = cxx_qt_lib::QPen;

        include!("cxx-qt-lib/qlist.h");
        type QList_QLineF = cxx_qt_lib::QList<cxx_qt_lib::QLineF>;

        include!("cxx-qt-lib/qvector.h");
        type QVector_QLineF = cxx_qt_lib::QVector<cxx_qt_lib::QLineF>;
        type QVector_f32 = cxx_qt_lib::QVector<f32>;

        include!("cxx-qt-lib/qpainter.h");
        type QPainter = cxx_qt_lib::QPainter;

        include!("cxx-qt-lib-shoop/qobject.h");
        type QObject = cxx_qt_lib_shoop::qobject::QObject;

        include!(<QtQuick/QQuickPaintedItem>);
        type QQuickPaintedItem;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QQuickPaintedItem]
        #[qproperty(i64, samples_offset)]
        #[qproperty(f64, samples_per_bin)]
        #[qproperty(QVariant, input_data)]
        #[qproperty(bool, have_data, READ=get_have_data, NOTIFY=have_data_changed)]
        type RenderAudioWaveform = super::RenderAudioWaveformRust;

        #[qinvokable]
        #[cxx_override]
        unsafe fn paint(self: Pin<&mut RenderAudioWaveform>, painter: *mut QPainter);

        #[inherit]
        fn size(self: &RenderAudioWaveform) -> QSizeF;

        #[inherit]
        fn update(self: Pin<&mut RenderAudioWaveform>);

        #[qinvokable]
        unsafe fn preprocess(self: Pin<&mut RenderAudioWaveform>);

        #[qinvokable]
        pub unsafe fn get_have_data(self: Pin<&mut RenderAudioWaveform>) -> bool;

        #[inherit]
        #[qsignal]
        #[cxx_name = "widthChanged"]
        fn width_changed(self: Pin<&mut RenderAudioWaveform>);

        #[qsignal]
        fn have_data_changed(self: Pin<&mut RenderAudioWaveform>);
    }

    impl cxx_qt::Constructor<()> for RenderAudioWaveform {}

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/make_unique.h");

        #[rust_name = "make_unique_renderaudiowaveform"]
        fn make_unique() -> UniquePtr<RenderAudioWaveform>;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/register_qml_type.h");

        #[rust_name = "register_qml_type_renderaudiowaveform"]
        unsafe fn register_qml_type(
            inference_example: *mut RenderAudioWaveform,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );
    }

    unsafe extern "RustQt" {
        #[qobject]
        type UpdateNotifier = super::UpdateNotifierRust;

        #[qsignal]
        pub unsafe fn notify_done(self: Pin<&mut UpdateNotifier>);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qobject.h");

        #[rust_name = "from_qobject_ref_update_notifier"]
        unsafe fn fromQObjectRef(obj: &QObject, output: *mut *const UpdateNotifier);

        #[rust_name = "from_qobject_mut_update_notifier"]
        unsafe fn fromQObjectMut(obj: Pin<&mut QObject>, output: *mut *mut UpdateNotifier);

        #[rust_name = "update_notifier_qobject_from_ptr"]
        unsafe fn qobjectFromPtr(obj: *mut UpdateNotifier) -> *mut QObject;

        #[rust_name = "update_notifier_qobject_from_ref"]
        fn qobjectFromRef(obj: &UpdateNotifier) -> &QObject;

        include!("cxx-qt-lib-shoop/make_raw.h");
        #[rust_name = "make_raw_update_notifier"]
        unsafe fn make_raw() -> *mut UpdateNotifier;

        include!("cxx-qt-lib-shoop/cast_ptr.h");
        #[rust_name = "update_notifier_to_qobject"]
        unsafe fn cast_ptr(obj: *mut UpdateNotifier) -> *mut QObject;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qobject.h");

        #[rust_name = "from_qobject_ref_render_audio_waveform"]
        unsafe fn fromQObjectRef(obj: &QObject, output: *mut *const RenderAudioWaveform);

        #[rust_name = "from_qobject_mut_render_audio_waveform"]
        unsafe fn fromQObjectMut(obj: Pin<&mut QObject>, output: *mut *mut RenderAudioWaveform);

        #[rust_name = "render_audio_waveform_qobject_from_ptr"]
        unsafe fn qobjectFromPtr(obj: *mut RenderAudioWaveform) -> *mut QObject;

        #[rust_name = "render_audio_waveform_qobject_from_ref"]
        fn qobjectFromRef(obj: &RenderAudioWaveform) -> &QObject;

        include!("cxx-qt-lib-shoop/make_raw.h");
        #[rust_name = "make_raw_render_audio_waveform"]
        unsafe fn make_raw() -> *mut RenderAudioWaveform;

        include!("cxx-qt-lib-shoop/cast_ptr.h");
        #[rust_name = "render_audio_waveform_to_qobject"]
        unsafe fn cast_ptr(obj: *mut RenderAudioWaveform) -> *mut QObject;
    }
}

use crate::audio_power_pyramid;
use ffi::*;
use std::sync::Arc;
use std::sync::Mutex;

pub struct RenderAudioWaveformRust {
    pub pyramid: Arc<Mutex<audio_power_pyramid::AudioPowerPyramidData>>,
    pub samples_offset: i64,
    pub samples_per_bin: f64,
    pub input_data: QVariant,
}

#[derive(Default)]
pub struct UpdateNotifierRust {}

impl Default for RenderAudioWaveformRust {
    fn default() -> RenderAudioWaveformRust {
        RenderAudioWaveformRust {
            pyramid: Arc::new(Mutex::new(
                audio_power_pyramid::AudioPowerPyramidData::default(),
            )),
            samples_offset: 0,
            samples_per_bin: 1_f64,
            input_data: QVariant::default(),
        }
    }
}
