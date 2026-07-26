//! Saving and loading a session.
//!
//! One JSON file with the samples inline. The existing application uses a tar of wav
//! files, which is the better format for large takes; this is deliberately the simplest
//! thing that round-trips, because the point here is that a session survives a restart,
//! not that it does so compactly. A minute of stereo audio in JSON is tens of megabytes,
//! so that trade wants revisiting before this carries real recordings.
//!
//! What is saved is what a user would be annoyed to lose: the audio in each loop, its
//! length, each track's gain and muting, and the instrument's settings. Transport state is
//! not saved -- reopening a session with loops already rolling would be surprising.

use serde::{Deserialize, Serialize};

/// A loop's contents, addressed by its position in the grid rather than by index.
///
/// By position because indices depend on the order things were created, which is an
/// implementation detail that should not be baked into a file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SavedLoop {
    pub track: usize,
    pub row: usize,
    pub length: u32,
    pub samples: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SavedTrack {
    pub gain: f32,
    pub muted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SavedInstrument {
    /// Stored by name, so reordering the waveform enum cannot silently change a session.
    pub waveform: String,
    pub gain: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SavedSession {
    /// Bumped when the format changes incompatibly, so an old file is refused rather than
    /// misread.
    pub version: u32,
    pub sample_rate: u32,
    pub sync_length: u32,
    pub tracks: Vec<SavedTrack>,
    pub loops: Vec<SavedLoop>,
    pub instrument: SavedInstrument,
}

pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum PersistError {
    #[error("could not read or write the file: {0}")]
    Io(#[from] std::io::Error),
    #[error("the file is not a session, or is damaged: {0}")]
    Format(#[from] serde_json::Error),
    #[error("session format version {found} is not supported (this build reads {supported})")]
    Version { found: u32, supported: u32 },
}

impl SavedSession {
    pub fn to_json(&self) -> Result<String, PersistError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Parses a session, refusing a version this build does not understand.
    pub fn from_json(text: &str) -> Result<Self, PersistError> {
        let parsed: SavedSession = serde_json::from_str(text)?;
        if parsed.version != FORMAT_VERSION {
            return Err(PersistError::Version {
                found: parsed.version,
                supported: FORMAT_VERSION,
            });
        }
        Ok(parsed)
    }

    pub fn save(&self, path: &std::path::Path) -> Result<(), PersistError> {
        std::fs::write(path, self.to_json()?)?;
        Ok(())
    }

    pub fn load(path: &std::path::Path) -> Result<Self, PersistError> {
        Self::from_json(&std::fs::read_to_string(path)?)
    }

    /// Loops that actually hold something, so an empty grid saves almost nothing.
    pub fn non_empty_loops(&self) -> impl Iterator<Item = &SavedLoop> {
        self.loops.iter().filter(|l| l.length > 0)
    }
}

/// Waveform to a stable name and back.
///
/// Explicit rather than derived so that renaming a variant is a deliberate format change
/// rather than an accidental one.
pub fn waveform_name(w: shoop_engine::wave_generator::Waveform) -> &'static str {
    use shoop_engine::wave_generator::Waveform as W;
    match w {
        W::Sine => "sine",
        W::Square => "square",
        W::Saw => "saw",
        W::Triangle => "triangle",
    }
}

/// The named waveform, falling back to sine for a name this build does not know.
///
/// Falling back rather than failing: an unknown waveform should not make a whole session
/// unloadable.
pub fn waveform_from_name(name: &str) -> shoop_engine::wave_generator::Waveform {
    use shoop_engine::wave_generator::Waveform as W;
    match name {
        "square" => W::Square,
        "saw" => W::Saw,
        "triangle" => W::Triangle,
        _ => W::Sine,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_engine::wave_generator::Waveform;

    fn sample_session() -> SavedSession {
        SavedSession {
            version: FORMAT_VERSION,
            sample_rate: 48000,
            sync_length: 96000,
            tracks: vec![
                SavedTrack {
                    gain: 1.0,
                    muted: false,
                },
                SavedTrack {
                    gain: 0.5,
                    muted: true,
                },
            ],
            loops: vec![
                SavedLoop {
                    track: 0,
                    row: 0,
                    length: 4,
                    samples: vec![0.1, 0.2, 0.3, 0.4],
                },
                SavedLoop {
                    track: 1,
                    row: 2,
                    length: 0,
                    samples: Vec::new(),
                },
            ],
            instrument: SavedInstrument {
                waveform: "saw".to_string(),
                gain: 0.3,
            },
        }
    }

    #[test]
    fn a_session_round_trips_through_json() {
        let original = sample_session();
        let text = original.to_json().expect("serialise");
        let back = SavedSession::from_json(&text).expect("parse");
        assert_eq!(original, back);
    }

    #[test]
    fn a_session_round_trips_through_a_file() {
        let dir = std::env::temp_dir().join("shoop_gui_persist_test");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("session.json");

        let original = sample_session();
        original.save(&path).expect("save");
        let back = SavedSession::load(&path).expect("load");
        assert_eq!(original, back);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_future_version_is_refused_rather_than_misread() {
        let mut s = sample_session();
        s.version = FORMAT_VERSION + 1;
        let text = s.to_json().expect("serialise");

        match SavedSession::from_json(&text) {
            Err(PersistError::Version { found, supported }) => {
                assert_eq!(found, FORMAT_VERSION + 1);
                assert_eq!(supported, FORMAT_VERSION);
            }
            other => panic!("expected a version error, got {other:?}"),
        }
    }

    #[test]
    fn nonsense_is_reported_as_a_format_error() {
        assert!(matches!(
            SavedSession::from_json("this is not json"),
            Err(PersistError::Format(_))
        ));
    }

    #[test]
    fn a_missing_file_is_reported_as_io() {
        let path = std::path::Path::new("/nonexistent/shoop/session.json");
        assert!(matches!(SavedSession::load(path), Err(PersistError::Io(_))));
    }

    #[test]
    fn empty_loops_are_distinguishable_from_full_ones() {
        let s = sample_session();
        let kept: Vec<_> = s.non_empty_loops().collect();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].track, 0);
    }

    #[test]
    fn waveform_names_round_trip() {
        for w in [
            Waveform::Sine,
            Waveform::Square,
            Waveform::Saw,
            Waveform::Triangle,
        ] {
            assert_eq!(waveform_from_name(waveform_name(w)), w);
        }
    }

    #[test]
    fn an_unknown_waveform_falls_back_rather_than_failing() {
        // A session written by a build with more waveforms should still load.
        assert_eq!(waveform_from_name("wavetable"), Waveform::Sine);
    }
}
