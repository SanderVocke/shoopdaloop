use crate::cxx_qt_shoop::qobj_loop_channel_backend_bridge::ffi::*;
use crate::{
    any_backend_channel::AnyBackendChannel, cxx_qt_shoop::qobj_loop_backend_bridge::LoopBackend,
};
use backend_bindings::{ChannelMode, MidiEvent, PortConnectability, PortDataType, PortDirection};
use common::logging::macros::{
    debug as raw_debug, error as raw_error, shoop_log_unit, trace as raw_trace,
};
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QList, QMap};
use cxx_qt_lib_shoop::{
    connect::connect_or_report,
    connection_types,
    qobject::{qobject_property_bool, AsQObject, FromQObject},
    qsharedpointer_qobject::QSharedPointer_QObject,
    qvariant_helpers::qvariant_to_qsharedpointer_qobject,
};
use std::{
    collections::{HashMap, HashSet},
    pin::Pin,
};
shoop_log_unit!("Frontend.LoopChannel");

macro_rules! trace {
    ($self:ident, $($arg:tt)*) => {
        raw_trace!("[{}-backend] {}", $self.display_name().to_string(), format!($($arg)*))
    };
}

macro_rules! debug {
    ($self:ident, $($arg:tt)*) => {
        raw_debug!("[{}-backend] {}", $self.display_name().to_string(), format!($($arg)*))
    };
}

macro_rules! error {
    ($self:ident, $($arg:tt)*) => {
        raw_error!("[{}-backend] {}", $self.display_name().to_string(), format!($($arg)*))
    };
}

impl LoopChannelBackend {
    pub fn update(mut self: Pin<&mut LoopChannelBackend>) {
        if self.maybe_backend_channel.is_none() {
            return;
        }

        if let Err(e) = || -> Result<(), anyhow::Error> {
            let channel = self.maybe_backend_channel.as_ref().unwrap();
            let prev_state = self.prev_state.clone();
            let new_state = channel.get_state()?;

            {
                let mut rust_mut = self.as_mut().rust_mut();
                rust_mut.prev_state = new_state.clone();
            }

            let new_played_back_sample_variant = match new_state.played_back_sample {
                Some(sample) => QVariant::from(&(sample as i32)),
                None => QVariant::default(),
            };

            // Self "state_changed" signal
            unsafe {
                let initialized = self.initialized;
                self.as_mut().state_changed(
                    initialized,
                    new_state.mode as i32,
                    new_state.length as i32,
                    new_state.start_offset,
                    new_played_back_sample_variant.clone(),
                    new_state.n_preplay_samples as i32,
                    new_state.data_dirty,
                    new_state.audio_gain as f64,
                    new_state.audio_output_peak as f64,
                    new_state.n_events_triggered as i32,
                    new_state.n_notes_active as i32,
                );
            }

            // Update individual field signals
            unsafe {
                if new_state.mode != prev_state.mode {
                    self.as_mut().mode_changed(new_state.mode as i32);
                }
                if new_state.length != prev_state.length {
                    self.as_mut().data_length_changed(new_state.length as i32);
                }
                if new_state.start_offset != prev_state.start_offset {
                    self.as_mut().start_offset_changed(new_state.start_offset);
                }
                if new_state.played_back_sample != prev_state.played_back_sample {
                    self.as_mut()
                        .last_played_sample_changed(new_played_back_sample_variant);
                }
                if new_state.n_preplay_samples != prev_state.n_preplay_samples {
                    self.as_mut()
                        .n_preplay_samples_changed(new_state.n_preplay_samples as i32);
                }
                if new_state.data_dirty != prev_state.data_dirty {
                    self.as_mut().data_dirty_changed(new_state.data_dirty);
                }
                if new_state.audio_gain != prev_state.audio_gain {
                    self.as_mut()
                        .audio_gain_changed(new_state.audio_gain as f64);
                }
                if new_state.audio_output_peak != prev_state.audio_output_peak {
                    self.as_mut()
                        .audio_output_peak_changed(new_state.audio_output_peak as f64);
                }
                if new_state.n_events_triggered != prev_state.n_events_triggered {
                    self.as_mut()
                        .midi_n_events_triggered_changed(new_state.n_events_triggered as i32);
                }
                if new_state.n_notes_active != prev_state.n_notes_active {
                    self.as_mut()
                        .midi_n_notes_active_changed(new_state.n_notes_active as i32);
                }
            }

            Ok(())
        }() {
            error!(self, "Could not update: {e}")
        }
    }

    pub fn display_name(self: &LoopChannelBackend) -> String {
        // TODO
        "unknown".to_string()
    }

    pub fn maybe_initialize_backend(mut self: Pin<&mut LoopChannelBackend>) -> bool {
        match || -> Result<bool, anyhow::Error> {
            if self.initialized {
                return Ok(true);
            }

            let mut non_ready_vars: HashSet<String> = HashSet::new();
            let mut channel_loop: Option<cxx::UniquePtr<QSharedPointer_QObject>> = None;
            unsafe {
                if self.backend.is_null() {
                    non_ready_vars.insert("backend".to_string());
                }
                if !self.backend.is_null()
                    && !qobject_property_bool(self.backend.as_ref().unwrap(), "ready")
                        .unwrap_or(false)
                {
                    non_ready_vars.insert("backend_ready".to_string());
                }
                if self.channel_loop.is_none() {
                    non_ready_vars.insert("channel_loop".to_string());
                } else {
                    channel_loop = Some(self.channel_loop.as_ref().unwrap().to_strong()?);
                    if channel_loop.as_ref().unwrap().data()?.is_null() {
                        non_ready_vars.insert("channel_loop (null)".to_string());
                    } else {
                        if !qobject_property_bool(
                            channel_loop.as_ref().unwrap().data()?.as_ref().unwrap(),
                            "initialized",
                        )
                        .unwrap_or(false)
                        {
                            non_ready_vars.insert("channel_loop initialized".to_string());
                        }
                    }
                }
                if self.data_type.is_none() {
                    non_ready_vars.insert("data_type".to_string());
                }
            }
            let initialize_condition: bool = !self.initialized && non_ready_vars.is_empty();

            if initialize_condition {
                unsafe {
                    let channel_loop =
                        LoopBackend::from_qobject_ref_ptr(channel_loop.as_ref().unwrap().data()?)?;
                    let channel_loop = channel_loop
                        .backend_loop
                        .as_ref()
                        .ok_or(anyhow::anyhow!("No backend loop in loop object"))?;
                    let mode = ChannelMode::try_from(self.prev_state.mode as i32)?;
                    let backend_channel = match self.data_type.unwrap() {
                        PortDataType::Audio => {
                            AnyBackendChannel::Audio(channel_loop.add_audio_channel(mode)?)
                        }
                        PortDataType::Midi => {
                            AnyBackendChannel::Midi(channel_loop.add_midi_channel(mode)?)
                        }
                        PortDataType::Any => {
                            return Err(anyhow::anyhow!("No specific port data type"));
                        }
                    };
                    let mut rust_mut = self.as_mut().rust_mut();
                    rust_mut.maybe_backend_channel = Some(backend_channel);
                }
                todo!();
            } else {
                trace!(
                    self,
                    "Not initializing backend yet. Non-ready variables: {non_ready_vars:?}"
                );
                return Ok(false);
            }
        }() {
            Ok(result) => result,
            Err(e) => {
                error!(self, "Could not initialize backend: {e}");
                false
            }
        }
    }
}
