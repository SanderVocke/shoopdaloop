//! Colors used by the egui interface.

use egui::Color32;

pub const DARK_BACKGROUND: Color32 = Color32::from_rgb(28, 27, 31);
pub const RAISED_BACKGROUND: Color32 = Color32::from_rgb(85, 85, 85);
pub const INPUT_ACTIVE_BACKGROUND: Color32 = Color32::from_rgb(92, 75, 78);
pub const SIDEBAR_BACKGROUND: Color32 = DARK_BACKGROUND;
pub const CONTROL_BACKGROUND: Color32 = Color32::from_rgb(34, 34, 34);
pub const HOVER_BACKGROUND: Color32 = Color32::from_rgb(82, 82, 86);
pub const FOREGROUND: Color32 = Color32::WHITE;
pub const COLORED_HIGHLIGHT: Color32 = Color32::from_rgb(244, 143, 177);
pub const MUTED_FOREGROUND: Color32 = Color32::from_rgb(128, 128, 128);
pub const MUTED_SLIDER_FILL: Color32 = Color32::from_gray(174);
pub const WARNING: Color32 = Color32::YELLOW;
pub const ERROR: Color32 = Color32::LIGHT_RED;
pub const STRONG_ERROR: Color32 = Color32::RED;
pub const SUCCESS: Color32 = Color32::LIGHT_GREEN;

pub const PLAY_ACTION: Color32 = Color32::from_rgb(0, 128, 0);
pub const PLAYING_STATE: Color32 = Color32::from_rgb(0, 170, 0);
pub const RECORD_ACTION: Color32 = Color32::RED;
pub const DRY_THROUGH_WET: Color32 = Color32::from_rgb(255, 165, 0);
pub const MIDI_ACTIVITY: Color32 = Color32::from_rgb(72, 156, 230);
pub const AUDIO_ACTIVITY: Color32 = Color32::from_rgb(0, 188, 212);

pub const LOOP_REGULAR_COMPOSITE: Color32 = Color32::from_rgb(156, 96, 112);
pub const LOOP_COMPOSITE_REFERENCE_EDGE: Color32 = Color32::from_rgb(196, 120, 141);
pub const LOOP_SCRIPT_COMPOSITE: Color32 = Color32::from_rgb(119, 170, 119);
pub const LOOP_AUDIO_BACKGROUND: Color32 = Color32::from_rgb(0, 0, 68);
pub const LOOP_RECORDING_BACKGROUND: Color32 = Color32::from_rgb(68, 0, 0);
pub const LOOP_PROGRESS_PLAYING: Color32 = Color32::from_rgb(0, 68, 0);
pub const LOOP_PROGRESS_PLAYING_DRY: Color32 = Color32::from_rgb(51, 51, 0);
pub const LOOP_PROGRESS_RECORDING: Color32 = Color32::from_rgb(102, 0, 0);
pub const LOOP_PROGRESS_RECORDING_DRY: Color32 = Color32::from_rgb(102, 51, 0);
pub const LOOP_PROGRESS_OTHER: Color32 = Color32::from_rgb(68, 68, 68);
pub const LOOP_TARGET_EDGE: Color32 = DRY_THROUGH_WET;
pub const LOOP_SELECTED_EDGE: Color32 = WARNING;
pub const LOOP_SYNC_MARKER: Color32 = WARNING;
pub const LOOP_CONTENT_EDGE: Color32 = Color32::from_gray(221);
pub const LOOP_CONTROL_HOVER: Color32 = Color32::from_rgb(105, 105, 110);

pub const METER_LEVEL: Color32 = Color32::from_rgb(82, 132, 158);
pub const DIAL_LABEL: Color32 = Color32::from_gray(180);
pub const WAVEFORM_BACKGROUND: Color32 = Color32::from_rgb(25, 25, 27);
pub const WAVEFORM_ZERO_LINE: Color32 = Color32::from_rgb(92, 48, 54);
pub const WAVEFORM_LOOP_REGION: Color32 = Color32::from_rgba_unmultiplied_const(28, 28, 128, 110);
pub const WAVEFORM_LINE: Color32 = Color32::from_rgb(235, 35, 55);
pub const WAVEFORM_PLAYHEAD: Color32 = Color32::from_rgb(80, 210, 100);
