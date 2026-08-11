use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::fmt::Display;
use std::rc::Rc;
use std::time::Duration;

use anyhow::anyhow;
use omnilua::{FromLua, Function, IntoLua, Lua, Table, Value, Variadic};
use regex::Regex;
use shoop_app_api::{
    DefaultRecordingAction, LoopId, LoopMode, TrackId, MAX_TRACK_GAIN_DB, MIN_TRACK_GAIN_DB,
};

use crate::midi::{
    MidiConnectionId, MidiControlService, MidiEndpoint, MidiEndpointDirection,
    MAX_MIDI_MESSAGE_BYTES, MIDI_QUEUE_CAPACITY,
};
use crate::{install_compatibility_value, KEY_CONSTANTS, MODIFIER_CONSTANTS};

pub const MAX_SCRIPT_CALLBACKS_PER_PUMP: usize = 256;
pub const LOOP_DONT_WAIT_FOR_SYNC: i64 = -1;
pub const LOOP_DONT_ALIGN_TO_SYNC_IMMEDIATELY: i64 = -1;

pub const CONTROL_FUNCTION_NAMES: &[&str] = &[
    "loop_count",
    "loop_get_all",
    "loop_get_which_selected",
    "loop_get_which_targeted",
    "loop_get_by_mode",
    "loop_get_mode",
    "loop_get_next_mode",
    "loop_get_next_mode_delay",
    "loop_get_length",
    "loop_get_by_track",
    "loop_transition",
    "loop_trigger",
    "loop_trigger_grab",
    "loop_get_gain",
    "loop_get_gain_fader",
    "loop_get_balance",
    "loop_record_n",
    "loop_record_with_targeted",
    "loop_set_gain",
    "loop_set_gain_fader",
    "loop_set_balance",
    "loop_select",
    "loop_target",
    "loop_clear",
    "loop_clear_all",
    "loop_untarget_all",
    "loop_toggle_targeted",
    "loop_toggle_selected",
    "loop_adopt_ringbuffers",
    "loop_compose_add_to_end",
    "loop_set_repeat_sync",
    "track_get_gain",
    "track_get_balance",
    "track_get_gain_fader",
    "track_get_input_gain",
    "track_get_input_gain_fader",
    "track_get_muted",
    "track_set_muted",
    "track_get_input_muted",
    "track_set_input_muted",
    "track_set_gain",
    "track_set_balance",
    "track_set_gain_fader",
    "track_set_input_gain",
    "track_set_input_gain_fader",
    "set_apply_n_cycles",
    "get_apply_n_cycles",
    "set_solo",
    "get_solo",
    "set_sync_active",
    "get_sync_active",
    "set_play_after_record",
    "get_play_after_record",
    "set_default_recording_action",
    "get_default_recording_action",
    "register_loop_event_cb",
    "register_global_event_cb",
    "register_keyboard_event_cb",
    "register_one_shot_timer_cb",
    "auto_open_device_specific_midi_control_input",
    "auto_open_device_specific_midi_control_output",
];

#[derive(Clone, Debug, PartialEq)]
pub struct ControlLoop {
    pub id: LoopId,
    pub coords: [i64; 2],
    pub mode: LoopMode,
    pub next_mode: Option<LoopMode>,
    pub next_mode_delay: Option<u32>,
    pub length: u32,
    pub gain: f32,
    pub balance: f32,
    pub selected: bool,
    pub targeted: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ControlTrack {
    pub id: TrackId,
    pub index: i64,
    pub output_gain_db: f32,
    pub output_balance: f32,
    pub output_muted: bool,
    pub input_gain_db: f32,
    pub input_muted: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ControlSnapshot {
    pub loops: Vec<ControlLoop>,
    pub tracks: Vec<ControlTrack>,
    pub apply_n_cycles: u32,
    pub solo: bool,
    pub sync_active: bool,
    pub play_after_record: bool,
    pub default_recording_action: DefaultRecordingAction,
}

impl Default for ControlSnapshot {
    fn default() -> Self {
        Self {
            loops: Vec::new(),
            tracks: Vec::new(),
            apply_n_cycles: 0,
            solo: false,
            sync_active: true,
            play_after_record: true,
            default_recording_action: DefaultRecordingAction::Record,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ControlOperation {
    Transition {
        loops: Vec<LoopId>,
        mode: LoopMode,
        cycles_delay: Option<u32>,
        align_to_sync_at: Option<u32>,
    },
    Trigger {
        loops: Vec<LoopId>,
        mode: LoopMode,
    },
    Grab {
        loops: Vec<LoopId>,
    },
    RecordN {
        loops: Vec<LoopId>,
        n_cycles: u32,
        cycles_delay: u32,
    },
    RecordWithTargeted {
        loops: Vec<LoopId>,
    },
    SetLoopGain {
        loops: Vec<LoopId>,
        gain: f32,
    },
    SetLoopBalance {
        loops: Vec<LoopId>,
        balance: f32,
    },
    SetLoopSelection {
        loops: Vec<LoopId>,
        selected: bool,
        clear_others: bool,
    },
    SetTarget {
        target: Option<LoopId>,
    },
    ClearLoops {
        loops: Vec<LoopId>,
    },
    AdoptRingbuffers {
        loops: Vec<LoopId>,
        reverse_cycle_start: u32,
        cycles_length: u32,
        go_to_cycle: u32,
        go_to_mode: LoopMode,
    },
    ComposeAddToEnd {
        target: LoopId,
        add: Vec<LoopId>,
        parallel: bool,
    },
    SetRepeatSync {
        loops: Vec<LoopId>,
        active: bool,
    },
    SetTrackGain {
        tracks: Vec<TrackId>,
        gain_db: f32,
    },
    SetTrackBalance {
        tracks: Vec<TrackId>,
        balance: f32,
    },
    SetTrackMuted {
        tracks: Vec<TrackId>,
        muted: bool,
    },
    SetTrackInputGain {
        tracks: Vec<TrackId>,
        gain_db: f32,
    },
    SetTrackInputMuted {
        tracks: Vec<TrackId>,
        muted: bool,
    },
    SetApplyNCycles(u32),
    SetSolo(bool),
    SetSyncActive(bool),
    SetPlayAfterRecord(bool),
    SetDefaultRecordingAction(DefaultRecordingAction),
}

#[derive(Default)]
pub struct ControlBridge {
    pub snapshot: ControlSnapshot,
    pub operations: Vec<ControlOperation>,
}

pub type SharedControlBridge = Rc<RefCell<ControlBridge>>;

#[derive(Clone, Debug, PartialEq)]
pub struct ScriptLoopEvent {
    pub coords: [i64; 2],
    pub event_type: i64,
    pub mode: LoopMode,
    pub length: u32,
    pub selected: bool,
    pub targeted: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScriptKeyEvent {
    pub event_type: i64,
    pub key: i64,
    pub modifiers: i64,
}

pub struct TimerRegistration {
    pub remaining: Duration,
    pub callback: Function,
}

pub struct MidiInputRegistration {
    pub regex_source: String,
    pub regex: Option<Regex>,
    pub callback: Function,
    pub connections: BTreeMap<String, MidiConnectionId>,
    pub retry_remaining: Duration,
    pub latest_error: Option<String>,
}

pub struct MidiOutputRegistration {
    pub regex_source: String,
    pub regex: Option<Regex>,
    pub connected_callback: Function,
    pub port: Table,
    pub rate_limit_hz: u32,
    pub elapsed_since_send: Duration,
    pub queue: Rc<RefCell<VecDeque<Vec<u8>>>>,
    pub connections: BTreeMap<String, MidiConnectionId>,
    pub retry_remaining: Duration,
    pub latest_error: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ScriptActivityDiagnostics {
    pub loop_callbacks: u32,
    pub global_callbacks: u32,
    pub keyboard_callbacks: u32,
    pub timers: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MidiRuleRuntimeDirection {
    Input,
    Output,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MidiEndpointRuntimeDiagnostics {
    pub id: String,
    pub name: String,
    pub connected: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MidiRuleRuntimeDiagnostics {
    pub direction: MidiRuleRuntimeDirection,
    pub pattern: String,
    pub matched_endpoints: Vec<String>,
    pub connected_endpoints: Vec<String>,
    pub endpoints: Vec<MidiEndpointRuntimeDiagnostics>,
    pub latest_error: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MidiRuntimeDiagnostics {
    pub rules: u32,
    pub connections: u32,
    pub dropped_messages: u32,
    pub errors: u32,
    pub rule_states: Vec<MidiRuleRuntimeDiagnostics>,
}

pub struct ScriptCallbacks {
    pub loop_events: Rc<RefCell<Vec<Function>>>,
    pub global_events: Rc<RefCell<Vec<Function>>>,
    pub keyboard_events: Rc<RefCell<Vec<Function>>>,
    pub timers: Rc<RefCell<Vec<TimerRegistration>>>,
    pub midi_inputs: Rc<RefCell<Vec<MidiInputRegistration>>>,
    pub midi_outputs: Rc<RefCell<Vec<MidiOutputRegistration>>>,
    pub midi_diagnostics: Rc<RefCell<MidiRuntimeDiagnostics>>,
}

impl ScriptCallbacks {
    pub fn new() -> Self {
        Self {
            loop_events: Rc::new(RefCell::new(Vec::new())),
            global_events: Rc::new(RefCell::new(Vec::new())),
            keyboard_events: Rc::new(RefCell::new(Vec::new())),
            timers: Rc::new(RefCell::new(Vec::new())),
            midi_inputs: Rc::new(RefCell::new(Vec::new())),
            midi_outputs: Rc::new(RefCell::new(Vec::new())),
            midi_diagnostics: Rc::new(RefCell::new(MidiRuntimeDiagnostics::default())),
        }
    }

    pub fn has_activity(&self) -> bool {
        let timers = self.timers.borrow();
        for registration in timers.iter() {
            let _ = (registration.remaining, &registration.callback);
        }
        let midi_inputs = self.midi_inputs.borrow();
        for registration in midi_inputs.iter() {
            let _ = (
                &registration.regex_source,
                &registration.regex,
                &registration.callback,
                &registration.connections,
                registration.retry_remaining,
                &registration.latest_error,
            );
        }
        let midi_outputs = self.midi_outputs.borrow();
        for registration in midi_outputs.iter() {
            let _ = (
                &registration.regex_source,
                &registration.regex,
                &registration.connected_callback,
                &registration.port,
                registration.rate_limit_hz,
                registration.elapsed_since_send,
                &registration.queue,
                &registration.connections,
                registration.retry_remaining,
                &registration.latest_error,
            );
        }
        !self.loop_events.borrow().is_empty()
            || !self.global_events.borrow().is_empty()
            || !self.keyboard_events.borrow().is_empty()
            || !timers.is_empty()
            || !midi_inputs.is_empty()
            || !midi_outputs.is_empty()
    }

    pub fn dispatch_loop_event(&self, lua: &Lua, event: &ScriptLoopEvent) -> Vec<String> {
        let callbacks = self.loop_events.borrow().clone();
        callbacks
            .into_iter()
            .filter_map(|callback| {
                let result = (|| -> omnilua::Result<()> {
                    let table = lua.create_table()?;
                    table.set("coords", create_sequence_from(lua, event.coords)?)?;
                    table.set("type", event.event_type)?;
                    table.set("mode", mode_value(event.mode))?;
                    table.set("length", event.length)?;
                    table.set("selected", event.selected)?;
                    table.set("targeted", event.targeted)?;
                    callback.call::<_, ()>(table)
                })();
                result.err().map(|error| error.to_string())
            })
            .collect()
    }

    pub fn dispatch_global_event(&self, lua: &Lua) -> Vec<String> {
        let callbacks = self.global_events.borrow().clone();
        callbacks
            .into_iter()
            .filter_map(|callback| {
                let result = (|| -> omnilua::Result<()> {
                    let table = lua.create_table()?;
                    table.set("type", 0)?;
                    callback.call::<_, ()>(table)
                })();
                result.err().map(|error| error.to_string())
            })
            .collect()
    }

    pub fn dispatch_key_event(&self, lua: &Lua, event: ScriptKeyEvent) -> Vec<String> {
        let callbacks = self.keyboard_events.borrow().clone();
        callbacks
            .into_iter()
            .filter_map(|callback| {
                let result = (|| -> omnilua::Result<()> {
                    let table = lua.create_table()?;
                    table.set("type", event.event_type)?;
                    table.set("key", event.key)?;
                    table.set("modifiers", event.modifiers)?;
                    callback.call::<_, ()>(table)
                })();
                result.err().map(|error| error.to_string())
            })
            .collect()
    }

    pub fn advance_timers(&self, elapsed: Duration) -> Vec<String> {
        let callbacks = {
            let mut timers = self.timers.borrow_mut();
            for timer in timers.iter_mut() {
                timer.remaining = timer.remaining.saturating_sub(elapsed);
            }
            let mut callbacks = Vec::new();
            let mut index = 0;
            while index < timers.len() && callbacks.len() < MAX_SCRIPT_CALLBACKS_PER_PUMP {
                if timers[index].remaining.is_zero() {
                    callbacks.push(timers.remove(index).callback);
                } else {
                    index += 1;
                }
            }
            callbacks
        };
        callbacks
            .into_iter()
            .filter_map(|callback| {
                callback
                    .call::<_, ()>(())
                    .err()
                    .map(|error| error.to_string())
            })
            .collect()
    }

    pub fn advance_midi(
        &self,
        lua: &Lua,
        service: &mut dyn MidiControlService,
        endpoints: &[MidiEndpoint],
        elapsed: Duration,
    ) -> Vec<String> {
        let mut errors = Vec::new();
        let mut dropped_messages = 0_u32;
        let mut remaining_callbacks = MAX_SCRIPT_CALLBACKS_PER_PUMP;
        for rule in self.midi_inputs.borrow_mut().iter_mut() {
            let matches = matching_endpoints(
                endpoints,
                rule.regex.as_ref(),
                MidiEndpointDirection::Output,
            );
            disconnect_stale(service, &mut rule.connections, &matches);
            rule.retry_remaining = rule.retry_remaining.saturating_sub(elapsed);
            if rule.retry_remaining.is_zero() {
                for endpoint in &matches {
                    if !rule.connections.contains_key(&endpoint.id) {
                        match service.connect_input(&endpoint.id) {
                            Ok(id) => {
                                rule.connections.insert(endpoint.id.clone(), id);
                            }
                            Err(error) => {
                                let error = error.to_string();
                                rule.latest_error = Some(error.clone());
                                errors.push(error);
                                rule.retry_remaining = Duration::from_millis(250);
                            }
                        }
                    }
                }
            }
            for id in rule.connections.values().copied().collect::<Vec<_>>() {
                if remaining_callbacks == 0 {
                    break;
                }
                dropped_messages = dropped_messages.saturating_add(service.take_dropped_input(id));
                match service.drain_input(id, remaining_callbacks) {
                    Ok(messages) => {
                        for message in messages {
                            remaining_callbacks = remaining_callbacks.saturating_sub(1);
                            match create_sequence_from(lua, message.into_iter().map(i64::from)) {
                                Ok(table) => {
                                    if let Err(error) = rule.callback.call::<_, ()>(table) {
                                        let error = error.to_string();
                                        rule.latest_error = Some(error.clone());
                                        errors.push(error);
                                    }
                                }
                                Err(error) => {
                                    let error = error.to_string();
                                    rule.latest_error = Some(error.clone());
                                    errors.push(error);
                                }
                            }
                        }
                    }
                    Err(error) => {
                        let error = error.to_string();
                        rule.latest_error = Some(error.clone());
                        errors.push(error);
                    }
                }
            }
        }
        for rule in self.midi_outputs.borrow_mut().iter_mut() {
            let matches =
                matching_endpoints(endpoints, rule.regex.as_ref(), MidiEndpointDirection::Input);
            disconnect_stale(service, &mut rule.connections, &matches);
            rule.retry_remaining = rule.retry_remaining.saturating_sub(elapsed);
            if rule.retry_remaining.is_zero() {
                for endpoint in &matches {
                    if remaining_callbacks == 0 {
                        break;
                    }
                    if !rule.connections.contains_key(&endpoint.id) {
                        match service.connect_output(&endpoint.id) {
                            Ok(id) => {
                                rule.connections.insert(endpoint.id.clone(), id);
                                remaining_callbacks = remaining_callbacks.saturating_sub(1);
                                if let Err(error) =
                                    rule.connected_callback.call::<_, ()>(rule.port.clone())
                                {
                                    let error = error.to_string();
                                    rule.latest_error = Some(error.clone());
                                    errors.push(error);
                                }
                            }
                            Err(error) => {
                                let error = error.to_string();
                                rule.latest_error = Some(error.clone());
                                errors.push(error);
                                rule.retry_remaining = Duration::from_millis(250);
                            }
                        }
                    }
                }
            }
            if rule.connections.is_empty() {
                rule.elapsed_since_send = Duration::ZERO;
                continue;
            }
            rule.elapsed_since_send = rule.elapsed_since_send.saturating_add(elapsed);
            let count = if rule.rate_limit_hz == 0 {
                rule.queue.borrow().len()
            } else {
                let interval = Duration::from_secs_f64(1.0 / f64::from(rule.rate_limit_hz));
                if rule.elapsed_since_send >= interval {
                    // Never catch up by flushing a burst after a delayed control pump. A positive
                    // limit is a real maximum, so the wall-clock interval starts again when the
                    // single message is handed to the MIDI service.
                    rule.elapsed_since_send = Duration::ZERO;
                    1
                } else {
                    0
                }
            };
            let queued = rule.queue.borrow().len();
            for _ in 0..count.min(queued) {
                let Some(message) = rule.queue.borrow_mut().pop_front() else {
                    break;
                };
                for id in rule.connections.values().copied() {
                    if let Err(error) = service.send(id, &message) {
                        let error = error.to_string();
                        rule.latest_error = Some(error.clone());
                        errors.push(error);
                    }
                }
            }
        }
        let mut diagnostics = self.midi_diagnostics.borrow_mut();
        diagnostics.rules = (self.midi_inputs.borrow().len() + self.midi_outputs.borrow().len())
            .try_into()
            .unwrap_or(u32::MAX);
        diagnostics.connections = self
            .midi_inputs
            .borrow()
            .iter()
            .map(|rule| rule.connections.len())
            .chain(
                self.midi_outputs
                    .borrow()
                    .iter()
                    .map(|rule| rule.connections.len()),
            )
            .sum::<usize>()
            .try_into()
            .unwrap_or(u32::MAX);
        let endpoint_label = |endpoint: &MidiEndpoint| {
            if endpoint.id == endpoint.name {
                endpoint.name.clone()
            } else {
                format!("{} [{}]", endpoint.name, endpoint.id)
            }
        };
        diagnostics.rule_states = self
            .midi_inputs
            .borrow()
            .iter()
            .map(|rule| {
                let matches = matching_endpoints(
                    endpoints,
                    rule.regex.as_ref(),
                    MidiEndpointDirection::Output,
                );
                MidiRuleRuntimeDiagnostics {
                    direction: MidiRuleRuntimeDirection::Input,
                    pattern: rule.regex_source.clone(),
                    matched_endpoints: matches.iter().map(|entry| endpoint_label(entry)).collect(),
                    connected_endpoints: matches
                        .iter()
                        .filter(|entry| rule.connections.contains_key(&entry.id))
                        .map(|entry| endpoint_label(entry))
                        .collect(),
                    endpoints: matches
                        .iter()
                        .map(|entry| MidiEndpointRuntimeDiagnostics {
                            id: entry.id.clone(),
                            name: entry.name.clone(),
                            connected: rule.connections.contains_key(&entry.id),
                        })
                        .collect(),
                    latest_error: rule.latest_error.clone(),
                }
            })
            .chain(self.midi_outputs.borrow().iter().map(|rule| {
                let matches = matching_endpoints(
                    endpoints,
                    rule.regex.as_ref(),
                    MidiEndpointDirection::Input,
                );
                MidiRuleRuntimeDiagnostics {
                    direction: MidiRuleRuntimeDirection::Output,
                    pattern: rule.regex_source.clone(),
                    matched_endpoints: matches.iter().map(|entry| endpoint_label(entry)).collect(),
                    connected_endpoints: matches
                        .iter()
                        .filter(|entry| rule.connections.contains_key(&entry.id))
                        .map(|entry| endpoint_label(entry))
                        .collect(),
                    endpoints: matches
                        .iter()
                        .map(|entry| MidiEndpointRuntimeDiagnostics {
                            id: entry.id.clone(),
                            name: entry.name.clone(),
                            connected: rule.connections.contains_key(&entry.id),
                        })
                        .collect(),
                    latest_error: rule.latest_error.clone(),
                }
            }))
            .collect();
        diagnostics.dropped_messages = diagnostics
            .dropped_messages
            .saturating_add(dropped_messages);
        diagnostics.errors = diagnostics
            .errors
            .saturating_add(errors.len().try_into().unwrap_or(u32::MAX));
        errors
    }

    pub fn disconnect_midi(&self, service: &mut dyn MidiControlService) {
        for rule in self.midi_inputs.borrow_mut().iter_mut() {
            for id in std::mem::take(&mut rule.connections).into_values() {
                service.disconnect(id);
            }
        }
        for rule in self.midi_outputs.borrow_mut().iter_mut() {
            for id in std::mem::take(&mut rule.connections).into_values() {
                service.disconnect(id);
            }
            rule.queue.borrow_mut().clear();
        }
    }

    pub fn activity_diagnostics(&self) -> ScriptActivityDiagnostics {
        ScriptActivityDiagnostics {
            loop_callbacks: self
                .loop_events
                .borrow()
                .len()
                .try_into()
                .unwrap_or(u32::MAX),
            global_callbacks: self
                .global_events
                .borrow()
                .len()
                .try_into()
                .unwrap_or(u32::MAX),
            keyboard_callbacks: self
                .keyboard_events
                .borrow()
                .len()
                .try_into()
                .unwrap_or(u32::MAX),
            timers: self.timers.borrow().len().try_into().unwrap_or(u32::MAX),
        }
    }

    pub fn has_midi_rules(&self) -> bool {
        !self.midi_inputs.borrow().is_empty() || !self.midi_outputs.borrow().is_empty()
    }

    pub fn midi_diagnostics(&self) -> MidiRuntimeDiagnostics {
        self.midi_diagnostics.borrow().clone()
    }
}

fn matching_endpoints<'a>(
    endpoints: &'a [MidiEndpoint],
    regex: Option<&Regex>,
    direction: MidiEndpointDirection,
) -> Vec<&'a MidiEndpoint> {
    let Some(regex) = regex else {
        return Vec::new();
    };
    endpoints
        .iter()
        .filter(|endpoint| endpoint.direction == direction && regex.is_match(&endpoint.name))
        .collect()
}

fn disconnect_stale(
    service: &mut dyn MidiControlService,
    connections: &mut BTreeMap<String, MidiConnectionId>,
    matches: &[&MidiEndpoint],
) {
    let stale = connections
        .keys()
        .filter(|id| !matches.iter().any(|endpoint| endpoint.id == **id))
        .cloned()
        .collect::<Vec<_>>();
    for id in stale {
        if let Some(connection) = connections.remove(&id) {
            service.disconnect(connection);
        }
    }
}

fn compile_endpoint_regex(source: &str) -> omnilua::Result<Option<Regex>> {
    if source.is_empty() {
        return Ok(None);
    }
    Regex::new(&format!("^(?:{source})$"))
        .map(Some)
        .map_err(|error| runtime_error(format!("invalid MIDI autoconnect regex: {error}")))
}

pub fn install_control_api(
    lua: &Lua,
    run_sandboxed: &Function,
    bridge: SharedControlBridge,
    callbacks: &ScriptCallbacks,
    mark_listening: Rc<dyn Fn()>,
) -> anyhow::Result<()> {
    let module = (|| -> omnilua::Result<Table> {
        let module = lua.create_table()?;
        let constants = lua.create_table()?;
        install_constants(&constants)?;
        module.set("constants", constants)?;
        install_loop_queries(lua, &module, &bridge)?;
        install_loop_mutations(lua, &module, &bridge)?;
        install_track_api(lua, &module, &bridge)?;
        install_global_api(lua, &module, &bridge)?;
        install_subscriptions(lua, &module, callbacks, mark_listening)?;
        Ok(module)
    })()
    .map_err(|error| anyhow!("could not install shoop_control API: {error}"))?;
    install_compatibility_value(run_sandboxed, "__shoop_control", module)
}

fn install_constants(constants: &Table) -> omnilua::Result<()> {
    for (name, value) in [
        ("LoopMode_Unknown", 0),
        ("LoopMode_Stopped", 1),
        ("LoopMode_Playing", 2),
        ("LoopMode_Recording", 3),
        ("LoopMode_Replacing", 4),
        ("LoopMode_PlayingDryThroughWet", 5),
        ("LoopMode_RecordingDryIntoWet", 6),
        ("LoopEventType_ModeChanged", 0),
        ("LoopEventType_LengthChanged", 1),
        ("LoopEventType_SelectedChanged", 2),
        ("LoopEventType_TargetedChanged", 3),
        ("LoopEventType_CoordsChanged", 4),
        ("GlobalEventType_GlobalControlChanged", 0),
        ("KeyEventType_Pressed", 0),
        ("KeyEventType_Released", 1),
        ("Loop_DontWaitForSync", LOOP_DONT_WAIT_FOR_SYNC),
        (
            "Loop_DontAlignToSyncImmediately",
            LOOP_DONT_ALIGN_TO_SYNC_IMMEDIATELY,
        ),
    ] {
        constants.set(name, value)?;
    }
    for &(name, value) in KEY_CONSTANTS.iter().chain(MODIFIER_CONSTANTS.iter()) {
        constants.set(name, value)?;
    }
    Ok(())
}

fn install_loop_queries(
    lua: &Lua,
    module: &Table,
    bridge: &SharedControlBridge,
) -> omnilua::Result<()> {
    set_fn(
        lua,
        module,
        "loop_count",
        bridge,
        |_lua, bridge, selector| {
            Ok(Value::Integer(
                select_loops(&bridge.snapshot, &selector)?.len() as i64,
            ))
        },
    )?;
    let bridge_ = Rc::clone(bridge);
    module.set(
        "loop_get_all",
        lua.create_function(move |lua, ()| coords_table(lua, &bridge_.borrow().snapshot.loops))?,
    )?;
    let bridge_ = Rc::clone(bridge);
    module.set(
        "loop_get_which_selected",
        lua.create_function(move |lua, ()| {
            let bridge = bridge_.borrow();
            coords_table(
                lua,
                &bridge
                    .snapshot
                    .loops
                    .iter()
                    .filter(|loop_| loop_.selected)
                    .cloned()
                    .collect::<Vec<_>>(),
            )
        })?,
    )?;
    let bridge_ = Rc::clone(bridge);
    module.set(
        "loop_get_which_targeted",
        lua.create_function(move |lua, ()| {
            let bridge = bridge_.borrow();
            match bridge.snapshot.loops.iter().find(|loop_| loop_.targeted) {
                Some(loop_) => Ok(Value::Table(single_coords(lua, loop_.coords)?)),
                None => Ok(Value::Nil),
            }
        })?,
    )?;
    let bridge_ = Rc::clone(bridge);
    module.set(
        "loop_get_by_mode",
        lua.create_function(move |lua, mode: i64| {
            let mode = parse_mode(mode)?;
            let bridge = bridge_.borrow();
            coords_table(
                lua,
                &bridge
                    .snapshot
                    .loops
                    .iter()
                    .filter(|loop_| loop_.mode == mode)
                    .cloned()
                    .collect::<Vec<_>>(),
            )
        })?,
    )?;
    set_loop_list_getter(lua, module, "loop_get_mode", bridge, |loop_| {
        Value::Integer(mode_value(loop_.mode))
    })?;
    set_loop_list_getter(lua, module, "loop_get_next_mode", bridge, |loop_| {
        loop_
            .next_mode
            .map(|mode| Value::Integer(mode_value(mode)))
            .unwrap_or(Value::Nil)
    })?;
    set_loop_list_getter(lua, module, "loop_get_next_mode_delay", bridge, |loop_| {
        loop_
            .next_mode_delay
            .map(|delay| Value::Integer(i64::from(delay)))
            .unwrap_or(Value::Nil)
    })?;
    set_loop_list_getter(lua, module, "loop_get_length", bridge, |loop_| {
        Value::Integer(i64::from(loop_.length))
    })?;
    let bridge_ = Rc::clone(bridge);
    module.set(
        "loop_get_by_track",
        lua.create_function(move |lua, track: i64| {
            let bridge = bridge_.borrow();
            coords_table(
                lua,
                &bridge
                    .snapshot
                    .loops
                    .iter()
                    .filter(|loop_| loop_.coords[0] == track)
                    .cloned()
                    .collect::<Vec<_>>(),
            )
        })?,
    )?;
    set_loop_list_getter(lua, module, "loop_get_gain", bridge, |loop_| {
        Value::Number(f64::from(loop_.gain))
    })?;
    set_loop_list_getter(lua, module, "loop_get_gain_fader", bridge, |loop_| {
        Value::Number(f64::from(gain_to_fader(loop_.gain)))
    })?;
    set_loop_list_getter(lua, module, "loop_get_balance", bridge, |loop_| {
        Value::Number(f64::from(loop_.balance))
    })?;
    Ok(())
}

fn install_loop_mutations(
    lua: &Lua,
    module: &Table,
    bridge: &SharedControlBridge,
) -> omnilua::Result<()> {
    let bridge_ = Rc::clone(bridge);
    module.set(
        "loop_transition",
        lua.create_function(
            move |lua, (selector, mode, rest): (Value, i64, Variadic<Value>)| {
                let [delay, align] = exact_arguments::<2>(rest, "loop_transition")?;
                let delay = i64::from_lua(delay, lua)?;
                let align = i64::from_lua(align, lua)?;
                let mut bridge = bridge_.borrow_mut();
                let ids = selected_loop_ids(&bridge.snapshot, &selector)?;
                let mode = parse_mode(mode)?;
                let cycles_delay = optional_u32(delay, LOOP_DONT_WAIT_FOR_SYNC)?;
                let align_to_sync_at = optional_u32(align, LOOP_DONT_ALIGN_TO_SYNC_IMMEDIATELY)?;
                shadow_transition(&mut bridge.snapshot, &ids, mode, cycles_delay);
                bridge.operations.push(ControlOperation::Transition {
                    loops: ids,
                    mode,
                    cycles_delay,
                    align_to_sync_at,
                });
                Ok(())
            },
        )?,
    )?;
    let bridge_ = Rc::clone(bridge);
    module.set(
        "loop_trigger",
        lua.create_function(move |_, (selector, mode): (Value, i64)| {
            let mut bridge = bridge_.borrow_mut();
            let ids = selected_loop_ids(&bridge.snapshot, &selector)?;
            let mode = parse_mode(mode)?;
            shadow_transition(&mut bridge.snapshot, &ids, mode, None);
            bridge
                .operations
                .push(ControlOperation::Trigger { loops: ids, mode });
            Ok(())
        })?,
    )?;
    set_loop_ids_op(lua, module, "loop_trigger_grab", bridge, |loops| {
        ControlOperation::Grab { loops }
    })?;
    let bridge_ = Rc::clone(bridge);
    module.set(
        "loop_record_n",
        lua.create_function(
            move |_, (selector, n_cycles, cycles_delay): (Value, u32, u32)| {
                let mut bridge = bridge_.borrow_mut();
                let loops = selected_loop_ids(&bridge.snapshot, &selector)?;
                bridge.operations.push(ControlOperation::RecordN {
                    loops,
                    n_cycles,
                    cycles_delay,
                });
                Ok(())
            },
        )?,
    )?;
    set_loop_ids_op(lua, module, "loop_record_with_targeted", bridge, |loops| {
        ControlOperation::RecordWithTargeted { loops }
    })?;
    set_loop_scalar(
        lua,
        module,
        "loop_set_gain",
        bridge,
        |bridge, ids, value| {
            let gain = value.max(0.0);
            for loop_ in loops_mut(&mut bridge.snapshot, &ids) {
                loop_.gain = gain;
            }
            ControlOperation::SetLoopGain { loops: ids, gain }
        },
    )?;
    set_loop_scalar(
        lua,
        module,
        "loop_set_gain_fader",
        bridge,
        |bridge, ids, value| {
            let gain = fader_to_gain(value);
            for loop_ in loops_mut(&mut bridge.snapshot, &ids) {
                loop_.gain = gain;
            }
            ControlOperation::SetLoopGain { loops: ids, gain }
        },
    )?;
    set_loop_scalar(
        lua,
        module,
        "loop_set_balance",
        bridge,
        |bridge, ids, value| {
            let balance = value.clamp(-1.0, 1.0);
            for loop_ in loops_mut(&mut bridge.snapshot, &ids) {
                loop_.balance = balance;
            }
            ControlOperation::SetLoopBalance {
                loops: ids,
                balance,
            }
        },
    )?;
    let bridge_ = Rc::clone(bridge);
    module.set(
        "loop_select",
        lua.create_function(move |_, (selector, clear): (Value, bool)| {
            let mut bridge = bridge_.borrow_mut();
            let ids = selected_loop_ids(&bridge.snapshot, &selector)?;
            if clear {
                for loop_ in &mut bridge.snapshot.loops {
                    loop_.selected = false;
                }
            }
            for loop_ in loops_mut(&mut bridge.snapshot, &ids) {
                loop_.selected = true;
            }
            bridge.operations.push(ControlOperation::SetLoopSelection {
                loops: ids,
                selected: true,
                clear_others: clear,
            });
            Ok(())
        })?,
    )?;
    install_target_functions(lua, module, bridge)?;
    set_loop_ids_op(lua, module, "loop_clear", bridge, |loops| {
        ControlOperation::ClearLoops { loops }
    })?;
    let bridge_ = Rc::clone(bridge);
    module.set(
        "loop_clear_all",
        lua.create_function(move |_, ()| {
            let mut bridge = bridge_.borrow_mut();
            let loops = bridge.snapshot.loops.iter().map(|loop_| loop_.id).collect();
            bridge
                .operations
                .push(ControlOperation::ClearLoops { loops });
            Ok(())
        })?,
    )?;
    let bridge_ = Rc::clone(bridge);
    module.set(
        "loop_adopt_ringbuffers",
        lua.create_function(
            move |lua, (selector, reverse, rest): (Value, u32, Variadic<Value>)| {
                let [length, go_cycle, go_mode] =
                    exact_arguments::<3>(rest, "loop_adopt_ringbuffers")?;
                let length = u32::from_lua(length, lua)?;
                let go_cycle = u32::from_lua(go_cycle, lua)?;
                let go_mode = i64::from_lua(go_mode, lua)?;
                let mut bridge = bridge_.borrow_mut();
                let loops = selected_loop_ids(&bridge.snapshot, &selector)?;
                bridge.operations.push(ControlOperation::AdoptRingbuffers {
                    loops,
                    reverse_cycle_start: reverse,
                    cycles_length: length,
                    go_to_cycle: go_cycle,
                    go_to_mode: parse_mode(go_mode)?,
                });
                Ok(())
            },
        )?,
    )?;
    let bridge_ = Rc::clone(bridge);
    module.set(
        "loop_compose_add_to_end",
        lua.create_function(move |_, (target, add, parallel): (Value, Value, bool)| {
            let mut bridge = bridge_.borrow_mut();
            let target = selected_loop_ids(&bridge.snapshot, &target)?
                .into_iter()
                .next()
                .ok_or_else(|| runtime_error("composition target is empty"))?;
            let add = selected_loop_ids(&bridge.snapshot, &add)?;
            bridge.operations.push(ControlOperation::ComposeAddToEnd {
                target,
                add,
                parallel,
            });
            Ok(())
        })?,
    )?;
    let bridge_ = Rc::clone(bridge);
    module.set(
        "loop_set_repeat_sync",
        lua.create_function(move |_, (selector, active): (Value, bool)| {
            let mut bridge = bridge_.borrow_mut();
            let loops = selected_loop_ids(&bridge.snapshot, &selector)?;
            bridge
                .operations
                .push(ControlOperation::SetRepeatSync { loops, active });
            Ok(())
        })?,
    )?;
    Ok(())
}

fn install_target_functions(
    lua: &Lua,
    module: &Table,
    bridge: &SharedControlBridge,
) -> omnilua::Result<()> {
    for name in ["loop_target", "loop_toggle_targeted"] {
        let bridge_ = Rc::clone(bridge);
        let toggle = name == "loop_toggle_targeted";
        module.set(
            name,
            lua.create_function(move |_, selector: Option<Value>| {
                let mut bridge = bridge_.borrow_mut();
                let target = match selector {
                    Some(selector) => selected_loop_ids(&bridge.snapshot, &selector)?
                        .into_iter()
                        .next(),
                    None => None,
                };
                let target = if toggle
                    && target.is_some()
                    && bridge
                        .snapshot
                        .loops
                        .iter()
                        .any(|loop_| loop_.id == target.unwrap() && loop_.targeted)
                {
                    None
                } else {
                    target
                };
                for loop_ in &mut bridge.snapshot.loops {
                    loop_.targeted = Some(loop_.id) == target;
                }
                bridge
                    .operations
                    .push(ControlOperation::SetTarget { target });
                Ok(())
            })?,
        )?;
    }
    let bridge_ = Rc::clone(bridge);
    module.set(
        "loop_untarget_all",
        lua.create_function(move |_, ()| {
            let mut bridge = bridge_.borrow_mut();
            for loop_ in &mut bridge.snapshot.loops {
                loop_.targeted = false;
            }
            bridge
                .operations
                .push(ControlOperation::SetTarget { target: None });
            Ok(())
        })?,
    )?;
    let bridge_ = Rc::clone(bridge);
    module.set(
        "loop_toggle_selected",
        lua.create_function(move |_, selector: Value| {
            let mut bridge = bridge_.borrow_mut();
            let ids = selected_loop_ids(&bridge.snapshot, &selector)?;
            for id in ids {
                let selected = bridge
                    .snapshot
                    .loops
                    .iter_mut()
                    .find(|loop_| loop_.id == id)
                    .map(|loop_| {
                        loop_.selected = !loop_.selected;
                        loop_.selected
                    });
                if let Some(selected) = selected {
                    bridge.operations.push(ControlOperation::SetLoopSelection {
                        loops: vec![id],
                        selected,
                        clear_others: false,
                    });
                }
            }
            Ok(())
        })?,
    )?;
    Ok(())
}

fn install_track_api(
    lua: &Lua,
    module: &Table,
    bridge: &SharedControlBridge,
) -> omnilua::Result<()> {
    set_track_list_getter(lua, module, "track_get_gain", bridge, |track| {
        Value::Number(f64::from(db_to_gain(track.output_gain_db)))
    })?;
    set_track_list_getter(lua, module, "track_get_balance", bridge, |track| {
        Value::Number(f64::from(track.output_balance))
    })?;
    set_track_list_getter(lua, module, "track_get_gain_fader", bridge, |track| {
        Value::Number(f64::from(db_to_fader(track.output_gain_db)))
    })?;
    set_track_list_getter(lua, module, "track_get_input_gain", bridge, |track| {
        Value::Number(f64::from(db_to_gain(track.input_gain_db)))
    })?;
    set_track_list_getter(lua, module, "track_get_input_gain_fader", bridge, |track| {
        Value::Number(f64::from(db_to_fader(track.input_gain_db)))
    })?;
    set_track_list_getter(lua, module, "track_get_muted", bridge, |track| {
        Value::Boolean(track.output_muted)
    })?;
    set_track_list_getter(lua, module, "track_get_input_muted", bridge, |track| {
        Value::Boolean(track.input_muted)
    })?;
    set_track_bool(lua, module, "track_set_muted", bridge, |tracks, muted| {
        ControlOperation::SetTrackMuted { tracks, muted }
    })?;
    set_track_bool(
        lua,
        module,
        "track_set_input_muted",
        bridge,
        |tracks, muted| ControlOperation::SetTrackInputMuted { tracks, muted },
    )?;
    set_track_number(lua, module, "track_set_gain", bridge, |tracks, gain| {
        ControlOperation::SetTrackGain {
            tracks,
            gain_db: gain_to_db(gain),
        }
    })?;
    set_track_number(
        lua,
        module,
        "track_set_gain_fader",
        bridge,
        |tracks, fader| ControlOperation::SetTrackGain {
            tracks,
            gain_db: fader_to_db(fader),
        },
    )?;
    set_track_number(
        lua,
        module,
        "track_set_balance",
        bridge,
        |tracks, balance| ControlOperation::SetTrackBalance {
            tracks,
            balance: balance.clamp(-1.0, 1.0),
        },
    )?;
    set_track_number(
        lua,
        module,
        "track_set_input_gain",
        bridge,
        |tracks, gain| ControlOperation::SetTrackInputGain {
            tracks,
            gain_db: gain_to_db(gain),
        },
    )?;
    set_track_number(
        lua,
        module,
        "track_set_input_gain_fader",
        bridge,
        |tracks, fader| ControlOperation::SetTrackInputGain {
            tracks,
            gain_db: fader_to_db(fader),
        },
    )?;
    Ok(())
}

fn install_global_api(
    lua: &Lua,
    module: &Table,
    bridge: &SharedControlBridge,
) -> omnilua::Result<()> {
    set_global_pair_u32(
        lua,
        module,
        bridge,
        "set_apply_n_cycles",
        "get_apply_n_cycles",
        |snapshot| snapshot.apply_n_cycles,
        |bridge, value| {
            bridge.snapshot.apply_n_cycles = value;
            ControlOperation::SetApplyNCycles(value)
        },
    )?;
    set_global_pair_bool(
        lua,
        module,
        bridge,
        "set_solo",
        "get_solo",
        |snapshot| snapshot.solo,
        |bridge, value| {
            bridge.snapshot.solo = value;
            ControlOperation::SetSolo(value)
        },
    )?;
    set_global_pair_bool(
        lua,
        module,
        bridge,
        "set_sync_active",
        "get_sync_active",
        |snapshot| snapshot.sync_active,
        |bridge, value| {
            bridge.snapshot.sync_active = value;
            ControlOperation::SetSyncActive(value)
        },
    )?;
    set_global_pair_bool(
        lua,
        module,
        bridge,
        "set_play_after_record",
        "get_play_after_record",
        |snapshot| snapshot.play_after_record,
        |bridge, value| {
            bridge.snapshot.play_after_record = value;
            ControlOperation::SetPlayAfterRecord(value)
        },
    )?;
    let bridge_ = Rc::clone(bridge);
    module.set(
        "set_default_recording_action",
        lua.create_function(move |_, value: String| {
            let action = match value.as_str() {
                "record" => DefaultRecordingAction::Record,
                "grab" => DefaultRecordingAction::Grab,
                _ => return Ok(()),
            };
            let mut bridge = bridge_.borrow_mut();
            bridge.snapshot.default_recording_action = action;
            bridge
                .operations
                .push(ControlOperation::SetDefaultRecordingAction(action));
            Ok(())
        })?,
    )?;
    let bridge_ = Rc::clone(bridge);
    module.set(
        "get_default_recording_action",
        lua.create_function(move |_, ()| {
            Ok(match bridge_.borrow().snapshot.default_recording_action {
                DefaultRecordingAction::Record => "record",
                DefaultRecordingAction::Grab => "grab",
            })
        })?,
    )?;
    Ok(())
}

fn install_subscriptions(
    lua: &Lua,
    module: &Table,
    callbacks: &ScriptCallbacks,
    mark_listening: Rc<dyn Fn()>,
) -> omnilua::Result<()> {
    for (name, storage) in [
        ("register_loop_event_cb", Rc::clone(&callbacks.loop_events)),
        (
            "register_global_event_cb",
            Rc::clone(&callbacks.global_events),
        ),
        (
            "register_keyboard_event_cb",
            Rc::clone(&callbacks.keyboard_events),
        ),
    ] {
        let mark_listening = Rc::clone(&mark_listening);
        module.set(
            name,
            lua.create_function(move |_, callback: Function| {
                storage.borrow_mut().push(callback);
                mark_listening();
                Ok(())
            })?,
        )?;
    }

    let timers = Rc::clone(&callbacks.timers);
    let timer_listening = Rc::clone(&mark_listening);
    module.set(
        "register_one_shot_timer_cb",
        lua.create_function(move |_, (delay_ms, callback): (i64, Function)| {
            let delay_ms = u64::try_from(delay_ms)
                .map_err(|_| runtime_error("timer delay may not be negative"))?;
            timers.borrow_mut().push(TimerRegistration {
                remaining: Duration::from_millis(delay_ms),
                callback,
            });
            timer_listening();
            Ok(())
        })?,
    )?;

    let midi_inputs = Rc::clone(&callbacks.midi_inputs);
    let input_listening = Rc::clone(&mark_listening);
    module.set(
        "auto_open_device_specific_midi_control_input",
        lua.create_function(move |_, (regex_source, callback): (String, Function)| {
            let regex = compile_endpoint_regex(&regex_source)?;
            midi_inputs.borrow_mut().push(MidiInputRegistration {
                regex_source,
                regex,
                callback,
                connections: BTreeMap::new(),
                retry_remaining: Duration::ZERO,
                latest_error: None,
            });
            input_listening();
            Ok(())
        })?,
    )?;

    let midi_outputs = Rc::clone(&callbacks.midi_outputs);
    let midi_diagnostics = Rc::clone(&callbacks.midi_diagnostics);
    let output_listening = Rc::clone(&mark_listening);
    module.set(
        "auto_open_device_specific_midi_control_output",
        lua.create_function(
            move |lua, (regex_source, opened_callback, rest): (String, Function, Variadic<Value>)| {
                let [connected_callback, rate_limit_hz] = exact_arguments::<2>(
                    rest,
                    "auto_open_device_specific_midi_control_output",
                )?;
                let connected_callback = Function::from_lua(connected_callback, lua)?;
                let rate_limit_hz = i64::from_lua(rate_limit_hz, lua)?;
                let rate_limit_hz = u32::try_from(rate_limit_hz).map_err(|_| {
                    runtime_error("MIDI output rate limit may not be negative")
                })?;
                let regex = compile_endpoint_regex(&regex_source)?;
                let queue = Rc::new(RefCell::new(VecDeque::new()));
                let send_queue = Rc::clone(&queue);
                let diagnostics = Rc::clone(&midi_diagnostics);
                let port = lua.create_table()?;
                port.set(
                    "send",
                    lua.create_function(move |_, message: Vec<i64>| {
                        if message.is_empty() || message.len() > MAX_MIDI_MESSAGE_BYTES {
                            return Err(runtime_error("invalid MIDI message length"));
                        }
                        let message = message
                            .into_iter()
                            .map(|byte| {
                                u8::try_from(byte).map_err(|_| {
                                    runtime_error("MIDI bytes must be between 0 and 255")
                                })
                            })
                            .collect::<omnilua::Result<Vec<_>>>()?;
                        let mut queue = send_queue.borrow_mut();
                        if queue.len() == MIDI_QUEUE_CAPACITY {
                            let mut diagnostics = diagnostics.borrow_mut();
                            diagnostics.dropped_messages =
                                diagnostics.dropped_messages.saturating_add(1);
                        } else {
                            queue.push_back(message);
                        }
                        Ok(())
                    })?,
                )?;
                opened_callback.call::<_, ()>(port.clone())?;
                midi_outputs.borrow_mut().push(MidiOutputRegistration {
                    regex_source,
                    regex,
                    connected_callback,
                    port,
                    rate_limit_hz,
                    elapsed_since_send: Duration::ZERO,
                    queue,
                    connections: BTreeMap::new(),
                    retry_remaining: Duration::ZERO,
                    latest_error: None,
                });
                output_listening();
                Ok(())
            },
        )?,
    )?;
    Ok(())
}

fn set_fn(
    lua: &Lua,
    module: &Table,
    name: &str,
    bridge: &SharedControlBridge,
    callback: impl Fn(&Lua, &ControlBridge, Value) -> omnilua::Result<Value> + 'static,
) -> omnilua::Result<()> {
    let bridge = Rc::clone(bridge);
    module.set(
        name,
        lua.create_function(move |lua, value: Value| callback(lua, &bridge.borrow(), value))?,
    )?;
    Ok(())
}

fn set_loop_list_getter(
    lua: &Lua,
    module: &Table,
    name: &str,
    bridge: &SharedControlBridge,
    getter: impl Fn(&ControlLoop) -> Value + 'static,
) -> omnilua::Result<()> {
    let bridge = Rc::clone(bridge);
    module.set(
        name,
        lua.create_function(move |lua, selector: Value| {
            let bridge = bridge.borrow();
            let table = lua.create_table()?;
            for (index, loop_) in select_loops(&bridge.snapshot, &selector)?
                .iter()
                .enumerate()
            {
                table.set(index + 1, getter(loop_))?;
            }
            Ok(table)
        })?,
    )?;
    Ok(())
}

fn set_track_list_getter(
    lua: &Lua,
    module: &Table,
    name: &str,
    bridge: &SharedControlBridge,
    getter: impl Fn(&ControlTrack) -> Value + 'static,
) -> omnilua::Result<()> {
    let bridge = Rc::clone(bridge);
    module.set(
        name,
        lua.create_function(move |lua, selector: Value| {
            let bridge = bridge.borrow();
            let table = lua.create_table()?;
            for (index, track) in select_tracks(&bridge.snapshot, &selector)?
                .iter()
                .enumerate()
            {
                table.set(index + 1, getter(track))?;
            }
            Ok(table)
        })?,
    )?;
    Ok(())
}

fn set_loop_ids_op(
    lua: &Lua,
    module: &Table,
    name: &str,
    bridge: &SharedControlBridge,
    make: impl Fn(Vec<LoopId>) -> ControlOperation + 'static,
) -> omnilua::Result<()> {
    let bridge = Rc::clone(bridge);
    module.set(
        name,
        lua.create_function(move |_, selector: Value| {
            let mut bridge = bridge.borrow_mut();
            let ids = selected_loop_ids(&bridge.snapshot, &selector)?;
            bridge.operations.push(make(ids));
            Ok(())
        })?,
    )?;
    Ok(())
}

fn set_loop_scalar(
    lua: &Lua,
    module: &Table,
    name: &str,
    bridge: &SharedControlBridge,
    make: impl Fn(&mut ControlBridge, Vec<LoopId>, f32) -> ControlOperation + 'static,
) -> omnilua::Result<()> {
    let bridge = Rc::clone(bridge);
    module.set(
        name,
        lua.create_function(move |_, (selector, value): (Value, f64)| {
            let mut bridge = bridge.borrow_mut();
            let ids = selected_loop_ids(&bridge.snapshot, &selector)?;
            let operation = make(&mut bridge, ids, value as f32);
            bridge.operations.push(operation);
            Ok(())
        })?,
    )?;
    Ok(())
}

fn set_track_bool(
    lua: &Lua,
    module: &Table,
    name: &str,
    bridge: &SharedControlBridge,
    make: impl Fn(Vec<TrackId>, bool) -> ControlOperation + 'static,
) -> omnilua::Result<()> {
    let bridge = Rc::clone(bridge);
    let operation_name = name.to_owned();
    module.set(
        name,
        lua.create_function(move |_, (selector, value): (Value, bool)| {
            let mut bridge = bridge.borrow_mut();
            let ids = selected_track_ids(&bridge.snapshot, &selector)?;
            shadow_track_bool(&mut bridge.snapshot, &ids, &operation_name, value);
            bridge.operations.push(make(ids, value));
            Ok(())
        })?,
    )?;
    Ok(())
}

fn set_track_number(
    lua: &Lua,
    module: &Table,
    name: &str,
    bridge: &SharedControlBridge,
    make: impl Fn(Vec<TrackId>, f32) -> ControlOperation + 'static,
) -> omnilua::Result<()> {
    let bridge = Rc::clone(bridge);
    module.set(
        name,
        lua.create_function(move |_, (selector, value): (Value, f64)| {
            let mut bridge = bridge.borrow_mut();
            let ids = selected_track_ids(&bridge.snapshot, &selector)?;
            let operation = make(ids.clone(), value as f32);
            shadow_track_operation(&mut bridge.snapshot, &operation);
            bridge.operations.push(operation);
            Ok(())
        })?,
    )?;
    Ok(())
}

fn set_global_pair_bool(
    lua: &Lua,
    module: &Table,
    bridge: &SharedControlBridge,
    setter: &str,
    getter: &str,
    get: impl Fn(&ControlSnapshot) -> bool + 'static,
    set: impl Fn(&mut ControlBridge, bool) -> ControlOperation + 'static,
) -> omnilua::Result<()> {
    let bridge_ = Rc::clone(bridge);
    module.set(
        getter,
        lua.create_function(move |_, ()| Ok(get(&bridge_.borrow().snapshot)))?,
    )?;
    let bridge_ = Rc::clone(bridge);
    module.set(
        setter,
        lua.create_function(move |_, value: bool| {
            let mut bridge = bridge_.borrow_mut();
            let operation = set(&mut bridge, value);
            bridge.operations.push(operation);
            Ok(())
        })?,
    )?;
    Ok(())
}

fn set_global_pair_u32(
    lua: &Lua,
    module: &Table,
    bridge: &SharedControlBridge,
    setter: &str,
    getter: &str,
    get: impl Fn(&ControlSnapshot) -> u32 + 'static,
    set: impl Fn(&mut ControlBridge, u32) -> ControlOperation + 'static,
) -> omnilua::Result<()> {
    let bridge_ = Rc::clone(bridge);
    module.set(
        getter,
        lua.create_function(move |_, ()| Ok(get(&bridge_.borrow().snapshot)))?,
    )?;
    let bridge_ = Rc::clone(bridge);
    module.set(
        setter,
        lua.create_function(move |_, value: u32| {
            let mut bridge = bridge_.borrow_mut();
            let operation = set(&mut bridge, value);
            bridge.operations.push(operation);
            Ok(())
        })?,
    )?;
    Ok(())
}

fn select_loops<'a>(
    snapshot: &'a ControlSnapshot,
    selector: &Value,
) -> omnilua::Result<Vec<&'a ControlLoop>> {
    let coords = parse_loop_selector(selector)?;
    Ok(coords
        .iter()
        .filter_map(|coords| snapshot.loops.iter().find(|loop_| loop_.coords == *coords))
        .collect())
}

fn selected_loop_ids(snapshot: &ControlSnapshot, selector: &Value) -> omnilua::Result<Vec<LoopId>> {
    Ok(select_loops(snapshot, selector)?
        .into_iter()
        .map(|loop_| loop_.id)
        .collect())
}

fn select_tracks<'a>(
    snapshot: &'a ControlSnapshot,
    selector: &Value,
) -> omnilua::Result<Vec<&'a ControlTrack>> {
    let indices = parse_track_selector(selector)?;
    Ok(indices
        .iter()
        .filter_map(|index| snapshot.tracks.iter().find(|track| track.index == *index))
        .collect())
}

fn selected_track_ids(
    snapshot: &ControlSnapshot,
    selector: &Value,
) -> omnilua::Result<Vec<TrackId>> {
    Ok(select_tracks(snapshot, selector)?
        .into_iter()
        .map(|track| track.id)
        .collect())
}

fn parse_loop_selector(selector: &Value) -> omnilua::Result<Vec<[i64; 2]>> {
    match selector {
        Value::Nil => Ok(Vec::new()),
        Value::Table(table) if table.len()? == 2 => {
            if let (Ok(x), Ok(y)) = (table.get::<_, i64>(1), table.get::<_, i64>(2)) {
                Ok(vec![[x, y]])
            } else {
                parse_multi_coords(table)
            }
        }
        Value::Table(table) => parse_multi_coords(table),
        other => Err(runtime_error(format!(
            "unsupported loop selector: {}",
            value_type_name(other)
        ))),
    }
}

fn parse_multi_coords(table: &Table) -> omnilua::Result<Vec<[i64; 2]>> {
    let mut result = Vec::with_capacity(table.len()? as usize);
    for index in 1..=table.len()? {
        let value: Table = table.get(index)?;
        if value.len()? != 2 {
            return Err(runtime_error("loop coordinate must have two values"));
        }
        result.push([value.get(1)?, value.get(2)?]);
    }
    Ok(result)
}

fn parse_track_selector(selector: &Value) -> omnilua::Result<Vec<i64>> {
    match selector {
        Value::Nil => Ok(Vec::new()),
        Value::Integer(index) => Ok(vec![*index]),
        Value::Table(table) => (1..=table.len()?).map(|index| table.get(index)).collect(),
        other => Err(runtime_error(format!(
            "unsupported track selector: {}",
            value_type_name(other)
        ))),
    }
}

fn coords_table(lua: &Lua, loops: &[ControlLoop]) -> omnilua::Result<Table> {
    let table = lua.create_table()?;
    for (index, loop_) in loops.iter().enumerate() {
        table.set(index + 1, single_coords(lua, loop_.coords)?)?;
    }
    Ok(table)
}

fn single_coords(lua: &Lua, coords: [i64; 2]) -> omnilua::Result<Table> {
    create_sequence_from(lua, coords)
}

fn loops_mut<'a>(
    snapshot: &'a mut ControlSnapshot,
    ids: &'a [LoopId],
) -> impl Iterator<Item = &'a mut ControlLoop> {
    snapshot
        .loops
        .iter_mut()
        .filter(move |loop_| ids.contains(&loop_.id))
}

fn shadow_transition(
    snapshot: &mut ControlSnapshot,
    ids: &[LoopId],
    mode: LoopMode,
    delay: Option<u32>,
) {
    for loop_ in loops_mut(snapshot, ids) {
        if delay == Some(0) || delay.is_none() {
            loop_.mode = mode;
        }
        loop_.next_mode = Some(mode);
        loop_.next_mode_delay = delay;
    }
}

fn shadow_track_bool(snapshot: &mut ControlSnapshot, ids: &[TrackId], name: &str, value: bool) {
    for track in snapshot
        .tracks
        .iter_mut()
        .filter(|track| ids.contains(&track.id))
    {
        match name {
            "track_set_muted" => track.output_muted = value,
            "track_set_input_muted" => track.input_muted = value,
            _ => {}
        }
    }
}

fn shadow_track_operation(snapshot: &mut ControlSnapshot, operation: &ControlOperation) {
    for track in &mut snapshot.tracks {
        match operation {
            ControlOperation::SetTrackGain { tracks, gain_db } if tracks.contains(&track.id) => {
                track.output_gain_db = *gain_db
            }
            ControlOperation::SetTrackBalance { tracks, balance } if tracks.contains(&track.id) => {
                track.output_balance = *balance
            }
            ControlOperation::SetTrackInputGain { tracks, gain_db }
                if tracks.contains(&track.id) =>
            {
                track.input_gain_db = *gain_db
            }
            _ => {}
        }
    }
}

fn parse_mode(value: i64) -> omnilua::Result<LoopMode> {
    match value {
        0 => Ok(LoopMode::Unknown),
        1 => Ok(LoopMode::Stopped),
        2 => Ok(LoopMode::Playing),
        3 => Ok(LoopMode::Recording),
        4 => Ok(LoopMode::Replacing),
        5 => Ok(LoopMode::PlayingDryThroughWet),
        6 => Ok(LoopMode::RecordingDryIntoWet),
        _ => Err(runtime_error(format!("invalid loop mode {value}"))),
    }
}

fn mode_value(mode: LoopMode) -> i64 {
    match mode {
        LoopMode::Unknown => 0,
        LoopMode::Stopped => 1,
        LoopMode::Playing => 2,
        LoopMode::Recording => 3,
        LoopMode::Replacing => 4,
        LoopMode::PlayingDryThroughWet => 5,
        LoopMode::RecordingDryIntoWet => 6,
    }
}

fn optional_u32(value: i64, sentinel: i64) -> omnilua::Result<Option<u32>> {
    if value == sentinel {
        Ok(None)
    } else {
        u32::try_from(value)
            .map(Some)
            .map_err(|_| runtime_error("cycle value must be non-negative or sentinel"))
    }
}

fn exact_arguments<const N: usize>(
    values: Variadic<Value>,
    function: &str,
) -> omnilua::Result<[Value; N]> {
    if values.len() != N {
        return Err(runtime_error(format!(
            "{function} expects {} arguments, got {}",
            N + 2,
            values.len() + 2
        )));
    }
    values
        .into_iter()
        .collect::<Vec<_>>()
        .try_into()
        .map_err(|_| {
            runtime_error(format!(
                "{function} argument conversion produced the wrong length"
            ))
        })
}

fn create_sequence_from<T>(lua: &Lua, values: impl IntoIterator<Item = T>) -> omnilua::Result<Table>
where
    T: IntoLua,
{
    let table = lua.create_table()?;
    for (index, value) in values.into_iter().enumerate() {
        table.set(index + 1, value)?;
    }
    Ok(table)
}

fn runtime_error(message: impl Display) -> omnilua::Error {
    omnilua::LuaError::runtime(format_args!("{message}")).into()
}

fn value_type_name(value: &Value) -> &'static str {
    match value {
        Value::Nil => "nil",
        Value::Boolean(_) => "boolean",
        Value::Integer(_) | Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Table(_) => "table",
        Value::Function(_) => "function",
        Value::UserData(_) => "userdata",
        Value::LightUserData(_) => "light userdata",
        Value::Thread(_) => "thread",
    }
}

pub fn gain_to_db(gain: f32) -> f32 {
    if gain <= 0.0 {
        MIN_TRACK_GAIN_DB
    } else {
        (20.0 * gain.log10()).clamp(MIN_TRACK_GAIN_DB, MAX_TRACK_GAIN_DB)
    }
}

pub fn db_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db.clamp(MIN_TRACK_GAIN_DB, MAX_TRACK_GAIN_DB) / 20.0)
}

pub fn db_to_fader(db: f32) -> f32 {
    ((db.clamp(MIN_TRACK_GAIN_DB, MAX_TRACK_GAIN_DB) - MIN_TRACK_GAIN_DB)
        / (MAX_TRACK_GAIN_DB - MIN_TRACK_GAIN_DB))
        .clamp(0.0, 1.0)
}

pub fn fader_to_db(fader: f32) -> f32 {
    MIN_TRACK_GAIN_DB + fader.clamp(0.0, 1.0) * (MAX_TRACK_GAIN_DB - MIN_TRACK_GAIN_DB)
}

pub fn gain_to_fader(gain: f32) -> f32 {
    db_to_fader(gain_to_db(gain))
}

pub fn fader_to_gain(fader: f32) -> f32 {
    db_to_gain(fader_to_db(fader))
}
