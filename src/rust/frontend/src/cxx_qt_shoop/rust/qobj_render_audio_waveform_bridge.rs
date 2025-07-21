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

        include!("cxx-qt-lib/qlinef.h");
        type QLineF = cxx_qt_lib::QLineF;

        include!("cxx-qt-lib/qpen.h");
        type QPen = cxx_qt_lib::QPen;

        include!("cxx-qt-lib/qlist.h");
        type QList_QLineF = cxx_qt_lib::QList<cxx_qt_lib::QLineF>;

        include!("cxx-qt-lib/qvector.h");
        type QVector_QLineF = cxx_qt_lib::QVector<cxx_qt_lib::QLineF>;

        include!("cxx-qt-lib/qpainter.h");
        type QPainter = cxx_qt_lib::QPainter;

        include!(<QtQuick/QQuickPaintedItem>);
        type QQuickPaintedItem;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QQuickPaintedItem]
        #[qproperty(i64, samples_offset)]
        #[qproperty(f64, samples_per_bin)]
        #[qproperty(QList_f64, input_data)]
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

        #[inherit]
        #[qsignal]
        #[cxx_name = "widthChanged"]
        fn width_changed(self: Pin<&mut RenderAudioWaveform>);
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
        fn register_qml_type(
            inference_example: &RenderAudioWaveform,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );
    }
}

use crate::audio_power_pyramid;
use ffi::*;
use std::sync::Mutex;

pub struct RenderAudioWaveformRust {
    pub pyramid: Mutex<audio_power_pyramid::AudioPowerPyramidData>,
    pub samples_offset: i64,
    pub samples_per_bin: f64,
    pub input_data: QList_f64,
}

impl Default for RenderAudioWaveformRust {
    fn default() -> RenderAudioWaveformRust {
        RenderAudioWaveformRust {
            pyramid: audio_power_pyramid::AudioPowerPyramidData::default().into(),
            samples_offset: 0,
            samples_per_bin: 1_f64,
            input_data: QList_f64::default(),
        }
    }
}
