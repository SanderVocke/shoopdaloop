//! A GUI to test-drive the Rust engine, with a built-in instrument.
//!
//! A library as well as a binary so the wiring can be tested without a window: the
//! interesting part is instrument-to-engine-to-device, and that needs no UI to exercise.

pub mod app;
pub mod click_track;
pub mod composite;
pub mod history;
pub mod instrument;
pub mod keyboard;
pub mod midi_control;
pub mod persist;
pub mod script;
pub mod selection;
pub mod session;
pub mod waveform;
