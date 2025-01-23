use common::logging::macros::*;
shoop_log_unit!("Frontend.RenderAudioWaveform");

pub use crate::cxx_qt_shoop::qobj_render_audio_waveform_bridge::ffi::RenderAudioWaveform;
use crate::cxx_qt_shoop::qobj_render_audio_waveform_bridge::ffi::*;
use crate::cxx_qt_shoop::qobj_render_audio_waveform_bridge::*;
use core::pin::Pin;
use crate::audio_power_pyramid;
use cxx_qt_lib::{QColor, QLine};
use crate::audio_power_pyramid::*;
use cxx_qt::CxxQtType;

pub fn register_qml_type(module_name : &str, type_name : &str) {
    let obj = make_unique_renderaudiowaveform();
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    register_qml_type_renderaudiowaveform(obj.as_ref().unwrap(), &mut mdl, 1, 0, &mut tp);
}

impl ffi::RenderAudioWaveform {
    pub fn preprocess(self: Pin<&mut Self>) {
        {
            let s : &RenderAudioWaveformRust = self.rust();
            let mut p = s.pyramid.lock().unwrap();
            debug!("Preprocessing input data with {} values", s.inputData.len());
            *p = audio_power_pyramid::create_audio_power_pyramid(
                s.inputData.iter().copied(), 2048);
        }
        self.update();
    }

    pub unsafe fn paint(self: Pin<&mut Self>, painter: *mut ffi::QPainter) {
        trace!("paint (offset {}, scale {})", self.samplesOffset, self.samplesPerBin);
        let s : &RenderAudioWaveformRust = self.rust();
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
                if subsampling_factor <= self.samplesPerBin as usize {
                    break;
                }
            }
        }
        let level : &AudioPowerPyramidLevelData = maybe_level.unwrap();

        // Calculate lines
        let n_lines = f64::ceil(self.size().width()) as usize;
        let n_samples = level.data.len();
        let mut lines : QVector_QLineF = QVector_QLineF::default();
        lines.reserve(n_lines as isize);
        lines.clear();
        trace!("  - {} line slots, {} samples", lines.len(), level.data.len());
        for i in 0..n_lines {
            let sample_idx = (i as f64 + (self.samplesOffset as f64 / self.samplesPerBin))
                * self.samplesPerBin / level.subsampling_factor as f64;
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
        let mut pen = QPen::default();
        pen.set_color(&color);
        let mut_painter = painter.as_mut().unwrap();
        let mut p : Pin<&mut QPainter> = Pin::new_unchecked(mut_painter);
        p.as_mut().set_pen(&pen);
        p.as_mut().draw_linefs(&lines);
        let mut center_line = QLine::default();
        center_line.set_line(0,
            (0.5*self.size().height()) as i32,
            self.size().width() as i32,
            (0.5*self.size().height()) as i32);
        p.draw_line(&center_line);
    }
}

impl cxx_qt::Initialize for RenderAudioWaveform {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().on_inputData_changed(|qobject| { qobject.preprocess(); }).release();
        self.as_mut().on_samplesOffset_changed(|qobject| qobject.update()).release();
        self.as_mut().on_samplesPerBin_changed(|qobject| qobject.update()).release();
        self.as_mut().on_widthChanged(|q_object| q_object.update()).release();
    }
}