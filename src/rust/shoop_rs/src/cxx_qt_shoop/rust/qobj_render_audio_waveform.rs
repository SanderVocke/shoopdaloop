use cxx_qt_lib::QColor;
use ffi::QList_QLineF;
use crate::audio_power_pyramid::*;
use crate::logging::macros::*;

const SHOOP_LOG_UNIT : &str = "Frontend.RenderAudioWaveform";

#[cxx_qt::bridge(cxx_file_stem="qobj_render_audio_waveform")]
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
    }

    unsafe extern "C++" {
        include!(<QtCore/QLineF>);
        type QLineF = crate::cxx_qt_lib_shoop::qlinef::QLineF;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qpen.h");
        type QPen = crate::cxx_qt_lib_shoop::qpen::QPen;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qlist.h");
        include!("cxx-qt-lib-shoop/qlinef.h");
        type QList_QLineF = crate::cxx_qt_lib_shoop::qlinef::QList_QLineF;
    }

    unsafe extern "C++" {
        type QPainter;
        include!("cxx-qt-lib/qlist.h");
        include!("cxx-qt-lib-shoop/qlinef.h");
        include!(<QtGui/QPainter>);

        #[rust_name = "set_pen"]
        fn setPen(self : Pin<&mut QPainter>, pen : &QPen);

        #[rust_name = "draw_lines"]
        fn drawLines(self : Pin<&mut QPainter>, points : &QList_QLineF);

        #[rust_name = "draw_line"]
        fn drawLine(self : Pin<&mut QPainter>, x1 : i32, y1 : i32, x2 : i32, y2 : i32);
    }

    unsafe extern "C++" {
        include!(<QtQuick/QQuickPaintedItem>);
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = "QQuickPaintedItem"]
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
        fn width_changed(self: Pin<&mut RenderAudioWaveform>);
    }

    impl cxx_qt::Constructor<()> for RenderAudioWaveform {}

    unsafe extern "C++" {
        include!("cxx-qt-shoop/make_unique.h");

        #[rust_name = "make_unique_renderaudiowaveform"]
        fn make_unique() -> UniquePtr<RenderAudioWaveform>;
    }

    unsafe extern "C++" {
        include!("cxx-qt-shoop/register_qml_type.h");

        #[rust_name = "register_qml_type_renderaudiowaveform"]
        fn register_qml_type(inference_example: &RenderAudioWaveform,
                                  module_name : &mut String,
                                  version_major : i64, version_minor : i64,
                                  type_name : &mut String);
    }
}

use core::pin::Pin;
use std::sync::Mutex;
use crate::audio_power_pyramid;
use ffi::*;

pub fn register_qml_type(module_name : &str, type_name : &str) {
    let obj = make_unique_renderaudiowaveform();
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    register_qml_type_renderaudiowaveform(obj.as_ref().unwrap(), &mut mdl, 1, 0, &mut tp);
}


pub struct RenderAudioWaveformRust {
    pyramid : Mutex<audio_power_pyramid::AudioPowerPyramidData>,
    samples_offset : i64,
    samples_per_bin : f64,
    input_data : QList_f64,
}

impl Default for RenderAudioWaveformRust {
    fn default() -> RenderAudioWaveformRust {
        RenderAudioWaveformRust {
            pyramid : audio_power_pyramid::AudioPowerPyramidData::default().into(),
            samples_offset : 0,
            samples_per_bin : 1_f64,
            input_data : QList_f64::default(),
        }
    }
}

impl ffi::RenderAudioWaveform {
    pub fn preprocess(self: Pin<&mut Self>) {
        {
            let s : &RenderAudioWaveformRust = self.cxx_qt_ffi_rust();
            let mut p = s.pyramid.lock().unwrap();
            info!("Input data has {} values", s.input_data.len());
            *p = audio_power_pyramid::create_audio_power_pyramid(
                s.input_data.iter().copied(), 2048);
        }
        self.update();
    }

    pub unsafe fn paint(self: Pin<&mut Self>, painter: *mut ffi::QPainter) {
        trace!("paint (offset {}, scale {})", self.samples_offset, self.samples_per_bin);
        let s : &RenderAudioWaveformRust = self.cxx_qt_ffi_rust();
        let p = s.pyramid.lock().unwrap();

        // Skip if no pyramid
        if p.levels.len() == 0 {
            trace!("paint: pyramid is empty");
            return;
        }

        // Select the nearest subsampling level
        let mut maybe_level : Option<&AudioPowerPyramidLevelData> = None;
        {
            for i in 0..p.levels.len() {
                let ii = p.levels.len() - 1 - i;
                let subsampling_factor = p.levels[ii].subsampling_factor;
                maybe_level = Some(&p.levels[ii]);
                if subsampling_factor <= self.samples_per_bin as usize {
                    break;
                }
            }
        }
        let level : &AudioPowerPyramidLevelData = maybe_level.unwrap();

        // Calculate lines
        let n_lines = f64::ceil(self.size().width()) as usize;
        let n_samples = level.data.len();
        let mut lines : QList_QLineF = QList_QLineF::default();
        lines.reserve(n_lines as isize);
        lines.clear();
        trace!("  - {} line slots, {} samples", lines.len(), level.data.len());
        for i in 0..n_lines {
            let sample_idx = (i as f64 + (self.samples_offset as f64 / self.samples_per_bin))
                * self.samples_per_bin / level.subsampling_factor as f64;
            let under_idx =
                i64::min(
                    n_samples as i64,
                    i64::max(-1, f64::floor(sample_idx) as i64)
                ) as usize;
            let over_idx =
                i64::min(
                    n_samples as i64,
                    i64::max(-1, f64::ceil(sample_idx) as i64)
                ) as usize;
            let under_sample : f32 =
                if (0..n_samples).contains(&under_idx) { level.data[under_idx] } else { 0_f32 };
            let over_sample : f32 =
                if (0..n_samples).contains(&over_idx) { level.data[over_idx] } else { 0_f32 };
            let sample = f32::max(under_sample, over_sample);
            let mut line : QLineF = QLineF::default();
            let h = self.size().height();
            if sample < 0_f32 {
                line.set_line(i as f64, 0.5*h,
                              i as f64, 0.5*h + 1.0);
            } else {
                line.set_line(i as f64, (0.5 - 0.5*sample as f64)*h,
                              i as f64, (0.5 + 0.5*sample as f64)*h);
            }
            lines.append(line);
        }

        let color = QColor::from_rgb(
            (0.7*255.0) as i32,
            0, 0
        );
        let mut pen = QPen::make_boxed();
        QPen::set_color(Pin::new_unchecked(pen.as_mut()), &color);
        let mut_painter = painter.as_mut().unwrap();
        let mut p : Pin<&mut QPainter> = Pin::new_unchecked(mut_painter);
        p.as_mut().set_pen(&pen);
        p.as_mut().draw_lines(&lines);
        p.draw_line(
            0,
            (0.5*self.size().height()) as i32,
            self.size().width() as i32,
            (0.5*self.size().height()) as i32);
    }
}

impl cxx_qt::Initialize for RenderAudioWaveform {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().on_input_data_changed(|qobject| { qobject.preprocess(); }).release();
        self.as_mut().on_samples_offset_changed(|qobject| qobject.update()).release();
        self.as_mut().on_samples_per_bin_changed(|qobject| qobject.update()).release();
        self.as_mut().on_width_changed(|q_object| q_object.update()).release();
    }
}