//! Colors used by the egui interface.

use egui::Color32;

pub const DARK_BACKGROUND: Color32 = Color32::from_rgb(28, 27, 31);
pub const RAISED_BACKGROUND: Color32 = Color32::from_rgb(85, 85, 85);
pub const SIDEBAR_BACKGROUND: Color32 = Color32::from_rgb(42, 42, 42);
pub const CONTROL_BACKGROUND: Color32 = Color32::from_rgb(34, 34, 34);
pub const FOREGROUND: Color32 = Color32::WHITE;
pub const COLORED_HIGHLIGHT: Color32 = Color32::from_rgb(244, 143, 177);
pub const MUTED_FOREGROUND: Color32 = Color32::from_rgb(128, 128, 128);
pub const WARNING: Color32 = Color32::YELLOW;
pub const ERROR: Color32 = Color32::LIGHT_RED;
pub const STRONG_ERROR: Color32 = Color32::RED;
pub const SUCCESS: Color32 = Color32::LIGHT_GREEN;

pub const PLAY_ACTION: Color32 = Color32::from_rgb(0, 128, 0);
pub const PLAYING_STATE: Color32 = Color32::from_rgb(0, 170, 0);
pub const RECORD_ACTION: Color32 = Color32::RED;
pub const DRY_THROUGH_WET: Color32 = Color32::from_rgb(255, 165, 0);
pub const MIDI_ACTIVITY: Color32 = Color32::CYAN;
pub const AUDIO_ACTIVITY: Color32 = Color32::from_rgb(0, 188, 212);

pub const LOOP_REGULAR_COMPOSITE: Color32 = Color32::from_rgb(255, 192, 203);
pub const LOOP_SCRIPT_COMPOSITE: Color32 = Color32::from_rgb(119, 170, 119);
pub const LOOP_AUDIO_BACKGROUND: Color32 = Color32::from_rgb(0, 0, 68);
pub const LOOP_PROGRESS_PLAYING: Color32 = Color32::from_rgb(0, 68, 0);
pub const LOOP_PROGRESS_PLAYING_DRY: Color32 = Color32::from_rgb(51, 51, 0);
pub const LOOP_PROGRESS_RECORDING: Color32 = Color32::from_rgb(102, 0, 0);
pub const LOOP_PROGRESS_RECORDING_DRY: Color32 = Color32::from_rgb(102, 51, 0);
pub const LOOP_PROGRESS_OTHER: Color32 = Color32::from_rgb(68, 68, 68);
pub const LOOP_TARGET_EDGE: Color32 = DRY_THROUGH_WET;
pub const LOOP_SELECTED_EDGE: Color32 = WARNING;
pub const LOOP_SYNC_MARKER: Color32 = WARNING;
pub const LOOP_CONTENT_EDGE: Color32 = Color32::from_gray(221);

pub const METER_LEVEL: Color32 = Color32::from_rgb(102, 102, 102);
pub const DIAL_LABEL: Color32 = Color32::from_gray(180);
pub const WAVEFORM_BACKGROUND: Color32 = Color32::from_rgb(24, 24, 24);
pub const WAVEFORM_ZERO_LINE: Color32 = Color32::from_gray(90);
pub const WAVEFORM_LOOP_REGION: Color32 = Color32::from_rgba_unmultiplied_const(0, 0, 180, 45);
pub const WAVEFORM_PLAYHEAD: Color32 = Color32::GREEN;
