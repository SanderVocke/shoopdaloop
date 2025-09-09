use common::logging::macros::*;
use cxx_qt_lib_shoop::qvariant_helpers::{
    qvariant_to_qsharedpointer_qvector_qvariant, qvariant_type_name,
};
shoop_log_unit!("Frontend.RenderAudioWaveform");

use crate::audio_power_pyramid;
use crate::audio_power_pyramid::*;
pub use crate::cxx_qt_shoop::qobj_render_audio_waveform_bridge::ffi::RenderAudioWaveform;
use crate::cxx_qt_shoop::qobj_render_audio_waveform_bridge::ffi::*;
use crate::cxx_qt_shoop::qobj_render_audio_waveform_bridge::*;
use core::pin::Pin;
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QColor, QLine};

pub fn register_qml_type(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe {
        register_qml_type_renderaudiowaveform(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp);
    }
}

impl ffi::RenderAudioWaveform {
    fn preprocess_from_iter(mut self: Pin<&mut Self>, iter: impl Iterator<Item = f32>) {
        {
            let mut p = self.pyramid.lock().unwrap();
            *p = audio_power_pyramid::create_audio_power_pyramid(iter, 2048);
            debug!("Created pyramid with {} levels", p.levels.len());
        }
        self.as_mut().update();
    }

    pub fn preprocess(mut self: Pin<&mut Self>) {
        if let Err(e) = || -> Result<(), anyhow::Error> {
            if let Ok(shared) = qvariant_to_qsharedpointer_qvector_qvariant(&self.input_data) {
                unsafe {
                    let vector = shared
                        .data()?
                        .as_ref()
                        .ok_or(anyhow::anyhow!("Could not dereference QVector pointer"))?;
                    debug!("Preprocessing input data - {} elements", vector.len());
                    self.as_mut().preprocess_from_iter(
                        vector.iter().map(|v| v.value::<f32>().unwrap_or(0.0)),
                    );
                }
            } else {
                return Err(anyhow::anyhow!(
                    "Unsupported input data type: {}",
                    qvariant_type_name(&self.input_data)?
                ));
            }

            Ok(())
        }() {
            error!("Could not preprocess input data: {e}")
        }
    }

    pub unsafe fn get_have_data(self: Pin<&mut RenderAudioWaveform>) -> bool {
        let pyramid = self.pyramid.lock().unwrap();
        pyramid.levels.len() > 0
    }

    pub unsafe fn paint(self: Pin<&mut Self>, painter: *mut ffi::QPainter) {
        trace!(
            "paint (offset {}, scale {})",
            self.samples_offset,
            self.samples_per_bin
        );
        let s: &RenderAudioWaveformRust = self.rust();
        let p = s.pyramid.lock().unwrap();

        // Skip if no pyramid
        if p.levels.len() == 0 {
            trace!("paint: pyramid is empty");
            return;
        }

        // Select the nearest subsampling level
        let mut maybe_level: Option<&AudioPowerPyramidLevelData> = None;
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
        let level: &AudioPowerPyramidLevelData = maybe_level.unwrap();

        // Calculate lines
        let n_lines = f64::ceil(self.size().width()) as usize;
        let n_samples = level.data.len();
        let mut lines: QVector_QLineF = QVector_QLineF::default();
        lines.reserve(n_lines as isize);
        lines.clear();
        trace!(
            "paint: {} line slots, {} samples",
            n_lines,
            level.data.len()
        );
        for i in 0..n_lines {
            let sample_idx = (i as f64 + (self.samples_offset as f64 / self.samples_per_bin))
                * self.samples_per_bin
                / level.subsampling_factor as f64;
            let under_idx = i64::min(
                n_samples as i64,
                i64::max(-1, f64::floor(sample_idx) as i64),
            ) as usize;
            let over_idx =
                i64::min(n_samples as i64, i64::max(-1, f64::ceil(sample_idx) as i64)) as usize;
            let under_sample: f32 = if (0..n_samples).contains(&under_idx) {
                level.data[under_idx]
            } else {
                0_f32
            };
            let over_sample: f32 = if (0..n_samples).contains(&over_idx) {
                level.data[over_idx]
            } else {
                0_f32
            };
            let sample = f32::max(under_sample, over_sample);
            let mut line: QLineF = QLineF::default();
            let h = self.size().height();
            if sample < 0_f32 {
                line.set_line(i as f64, 0.5 * h, i as f64, 0.5 * h + 1.0);
            } else {
                line.set_line(
                    i as f64,
                    (0.5 - 0.5 * sample as f64) * h,
                    i as f64,
                    (0.5 + 0.5 * sample as f64) * h,
                );
            }
            lines.append(line);
        }

        let color = QColor::from_rgb((0.7 * 255.0) as i32, 0, 0);
        let mut pen = QPen::default();
        pen.set_color(&color);
        let mut_painter = painter.as_mut().unwrap();
        let mut p: Pin<&mut QPainter> = Pin::new_unchecked(mut_painter);
        p.as_mut().set_pen(&pen);
        p.as_mut().draw_linefs(&lines);
        let mut center_line = QLine::default();
        center_line.set_line(
            0,
            (0.5 * self.size().height()) as i32,
            self.size().width() as i32,
            (0.5 * self.size().height()) as i32,
        );
        p.draw_line(&center_line);
    }
}

impl cxx_qt::Initialize for RenderAudioWaveform {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut()
            .on_input_data_changed(|qobject| {
                qobject.preprocess();
            })
            .release();
        self.as_mut()
            .on_samples_offset_changed(|qobject| qobject.update())
            .release();
        self.as_mut()
            .on_samples_per_bin_changed(|qobject| qobject.update())
            .release();
        self.as_mut()
            .on_width_changed(|q_object| q_object.update())
            .release();
    }
}
