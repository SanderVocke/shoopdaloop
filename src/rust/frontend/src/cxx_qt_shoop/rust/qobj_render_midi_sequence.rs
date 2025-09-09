use std::pin::Pin;

use backend_bindings::MidiEvent;
use common::logging::macros::*;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QRectF;
use cxx_qt_lib_shoop::qvariant_helpers::qvariant_to_qsharedpointer_qvector_qvariant;
use midi_processing::msgs_to_notes;
shoop_log_unit!("Frontend.RenderMidiSequence");

use crate::{
    cxx_qt_shoop::qobj_render_midi_sequence_bridge::{ffi::*, Note},
    midi_event_helpers::MidiEventToQVariant,
};

pub fn register_qml_type(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe {
        register_qml_type_rendermidisequence(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp);
    }
}

impl RenderMidiSequence {
    pub unsafe fn paint(mut self: Pin<&mut RenderMidiSequence>, painter: *mut QPainter) {
        trace!(
            "paint (offset {}, scale {})",
            self.samples_offset,
            self.samples_per_bin
        );
        if self.notes.len() == 0 {
            trace!("paint: no notes");
            return;
        }
        if painter.as_ref().is_none() {
            error!("paint: null painter");
            return;
        }
        let mut painter = std::pin::Pin::new_unchecked(&mut *painter);

        let width: i64 = self.size().width() as i64;
        let height: f64 = self.size().height();
        let mut rust_mut = self.as_mut().rust_mut();

        let samples_offset = rust_mut.samples_offset;
        let samples_per_bin = rust_mut.samples_per_bin;
        for note in rust_mut.notes.iter_mut() {
            note.scaled_start = std::cmp::max(
                0,
                ((note.start - samples_offset as i64) as f32 / samples_per_bin) as i64,
            );
            note.scaled_end = std::cmp::min(
                width,
                ((note.end - samples_offset as i64) as f32 / samples_per_bin) as i64,
            );
        }

        let filtered_notes_iter = || {
            rust_mut
                .notes
                .iter()
                .filter(|n| n.scaled_start < width && n.scaled_end > 0)
        };

        let mut min: u8 = 127;
        let mut max: u8 = 0;
        filtered_notes_iter().for_each(|n| {
            min = std::cmp::min(min, n.note);
            max = std::cmp::max(max, n.note);
        });
        let maxmindiff = std::cmp::max(1, max - min) as f64;

        let note_thickness =
            std::cmp::min(15, std::cmp::max(3, (height * 0.8 / maxmindiff) as i32)) as f32;

        let color = QColor::from_rgb(0, 0, (0.8 * 255.0) as i32);

        for note in filtered_notes_iter() {
            let y_rel: f32 = (note.note as f32 - min as f32) / maxmindiff as f32;
            let y_inv: f32 = (height as f32 * 0.1) + y_rel * (height as f32 * 0.8);
            let y = height as f32 - y_inv;

            let rect = QRectF::new(
                note.scaled_start as f64,
                (y - note_thickness / 2.0) as f64,
                (note.scaled_end - note.scaled_start) as f64,
                note_thickness as f64,
            );

            painter.as_mut().fill_rect(&rect, &color);
        }
    }

    pub unsafe fn parse(mut self: Pin<&mut RenderMidiSequence>) {
        if let Err(e) = || -> Result<(), anyhow::Error> {
            let shared = qvariant_to_qsharedpointer_qvector_qvariant(&self.input_data)?;
            let vector = shared
                .data()?
                .as_ref()
                .ok_or(anyhow::anyhow!("Could not extract data vector"))?;
            trace!("Got {} events", vector.len());
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.notes =
                msgs_to_notes(vector.iter().map(|v| MidiEvent::from_qvariant(&v).unwrap()))
                    .iter()
                    .map(|note| Note {
                        start: note.start_t as i64,
                        end: note.end_t as i64,
                        note: note.note,
                        scaled_start: 0,
                        scaled_end: 0,
                    })
                    .collect();

            trace!("{} msgs -> {} notes", vector.len(), rust_mut.notes.len());

            self.as_mut().update();
            Ok(())
        }() {
            error!("Could not parse: {e}");
        }
    }

    pub unsafe fn get_have_data(self: Pin<&mut RenderMidiSequence>) -> bool {
        self.notes.len() > 0
    }
}

impl cxx_qt::Initialize for RenderMidiSequence {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().on_width_changed(|s| s.update()).release();
        self.as_mut()
            .on_samples_offset_changed(|s| s.update())
            .release();
        self.as_mut()
            .on_samples_per_bin_changed(|s| s.update())
            .release();
        self.as_mut()
            .on_input_data_changed(|s| unsafe { s.parse() })
            .release();
    }
}
