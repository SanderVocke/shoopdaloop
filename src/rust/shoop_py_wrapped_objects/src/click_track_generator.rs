use cxx_qt_lib;
use cxx_qt_lib::{QList, QString};
use pyo3::prelude::*;
use pyo3::types::PyAny;

use common::logging::macros::*;
shoop_log_unit!("Frontend.ClickTrackGenerator");

pub struct ClickTrackGenerator {
    py_object: Option<Py<PyAny>>,
}

fn create_py_click_track_generator<'py>(py: Python<'py>) -> PyResult<Py<PyAny>> {
    let module = py
        .import("shoopdaloop.lib.q_objects.ClickTrackGeneratorImpl")
        .unwrap();
    let class = module
        .getattr("ClickTrackGeneratorImpl")
        .unwrap()
        .extract::<Py<PyAny>>()
        .unwrap();
    let object = class.call0(py).unwrap();
    Ok(object)
}

impl Default for ClickTrackGenerator {
    fn default() -> Self {
        Python::with_gil(|py| {
            return ClickTrackGenerator {
                py_object: Some(create_py_click_track_generator(py).unwrap()),
            };
        })
    }
}

impl ClickTrackGenerator {
    pub fn get_possible_clicks(&self) -> QList<QString> {
        match Python::with_gil(|py| -> PyResult<QList<QString>> {
            let mut rval: QList<QString> = QList::default();
            let result: Vec<String> = self
                .py_object
                .as_ref()
                .unwrap()
                .getattr(py, "get_possible_clicks")
                .unwrap()
                .call0(py)
                .unwrap()
                .extract(py)
                .unwrap();
            for elem in result {
                rval.append(QString::from(elem));
            }
            Ok(rval)
        }) {
            Ok(result) => result,
            Err(e) => {
                error!("Failed to get possible clicks, ignoring: {e}");
                QList::default()
            }
        }
    }

    pub fn generate_audio(
        &self,
        click_names: QList<QString>,
        bpm: i32,
        n_beats: i32,
        alt_click_delay_percent: i32,
    ) -> QString {
        match Python::with_gil(|py| -> PyResult<QString> {
            let click_names: Vec<String> = click_names.iter().map(|q| q.to_string()).collect();
            let args = (click_names, bpm, n_beats, alt_click_delay_percent)
                .into_pyobject(py)
                .unwrap();
            let result: String = self
                .py_object
                .as_ref()
                .unwrap()
                .getattr(py, "generate_audio")
                .unwrap()
                .call(py, args, None)
                .unwrap()
                .extract(py)
                .unwrap();
            Ok(QString::from(result))
        }) {
            Ok(result) => result,
            Err(e) => {
                error!("Failed to generate audio, ignoring: {e}");
                QString::default()
            }
        }
    }

    pub fn generate_midi(
        &self,
        notes: QList<i32>,
        channels: QList<i32>,
        velocities: QList<i32>,
        note_length: f32,
        bpm: i32,
        n_beats: i32,
        alt_click_delay_percent: i32,
    ) -> QString {
        match Python::with_gil(|py| -> PyResult<QString> {
            let notes: Vec<i32> = notes.iter().copied().collect();
            let channels: Vec<i32> = channels.iter().copied().collect();
            let velocities: Vec<i32> = velocities.iter().copied().collect();
            let args = (
                notes,
                channels,
                velocities,
                note_length,
                bpm,
                n_beats,
                alt_click_delay_percent,
            )
                .into_pyobject(py)
                .unwrap();
            let result: String = self
                .py_object
                .as_ref()
                .unwrap()
                .getattr(py, "generate_midi")
                .unwrap()
                .call(py, args, None)
                .unwrap()
                .extract(py)
                .unwrap();
            Ok(QString::from(result))
        }) {
            Ok(result) => result,
            Err(e) => {
                error!("Failed to generate midi, ignoring: {e}");
                QString::default()
            }
        }
    }

    pub fn preview(&self, wav_filename: QString) {
        match Python::with_gil(|py| -> PyResult<()> {
            let args = (wav_filename.to_string(),);
            self.py_object
                .as_ref()
                .unwrap()
                .getattr(py, "preview")
                .unwrap()
                .call(py, args, None)
                .unwrap();
            Ok(())
        }) {
            Ok(()) => (),
            Err(e) => {
                error!("Failed to preview wav, ignoring: {e}");
            }
        }
    }
}
