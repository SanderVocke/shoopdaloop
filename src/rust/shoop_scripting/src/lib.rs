use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::rc::Rc;

use anyhow::{anyhow, bail};
use mlua::{Function, Lua, Value};
use shoop_app_api::{ScriptId, ScriptKind, ScriptLifecycle, ScriptMidiDiagnostics, ScriptState};

mod control;
mod legacy_key_constants;
mod midi;

use control::{install_control_api, ScriptCallbacks};
pub use control::{
    ControlBridge, ControlLoop, ControlOperation, ControlSnapshot, ControlTrack,
    MidiRuntimeDiagnostics, ScriptKeyEvent, ScriptLoopEvent, SharedControlBridge,
    CONTROL_FUNCTION_NAMES,
};
use legacy_key_constants::{LEGACY_KEY_CONSTANTS, LEGACY_MODIFIER_CONSTANTS};
pub use midi::{
    FakeMidiControl, FakeMidiService, MidiConnectionId, MidiControlService, MidiEndpoint,
    MidiEndpointDirection, MidiEndpointSnapshot, NativeMidiService, NullMidiService,
    MAX_MIDI_MESSAGE_BYTES, MIDI_QUEUE_CAPACITY,
};

pub const KEYBOARD_SCRIPT: &str = include_str!("../../../lua/builtins/keyboard.lua");
pub const AKAI_APC_MINI_MK1_SCRIPT: &str =
    include_str!("../../../lua/builtins/akai_apc_mini_mk1.lua");
const SANDBOX_SOURCE: &str = include_str!("../../../lua/system/sandbox.lua");
const MAX_LOG_ENTRIES: usize = 100;

pub const BUILTIN_LIBRARIES: &[(&str, &str)] = &[
    (
        "shoop_control",
        include_str!("../../../lua/lib/shoop_control.lua"),
    ),
    (
        "shoop_coords",
        include_str!("../../../lua/lib/shoop_coords.lua"),
    ),
    (
        "shoop_format",
        include_str!("../../../lua/lib/shoop_format.lua"),
    ),
    (
        "shoop_helpers",
        include_str!("../../../lua/lib/shoop_helpers.lua"),
    ),
    (
        "shoop_midi",
        include_str!("../../../lua/lib/shoop_midi.lua"),
    ),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompatibilityPrintLevel {
    Trace,
    Debug,
    Info,
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScriptLogLevel {
    Trace,
    Debug,
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScriptLogEntry {
    pub level: ScriptLogLevel,
    pub message: String,
}

pub struct LuaRuntime {
    lua: Lua,
    run_sandboxed: Function,
    logs: Rc<RefCell<VecDeque<ScriptLogEntry>>>,
    listening: Rc<Cell<bool>>,
    callbacks: ScriptCallbacks,
}

impl LuaRuntime {
    pub fn new() -> anyhow::Result<Self> {
        Self::new_with_control(Rc::new(RefCell::new(ControlBridge::default())))
    }

    pub fn new_with_control(bridge: SharedControlBridge) -> anyhow::Result<Self> {
        let lua = Lua::new();
        let logs = Rc::new(RefCell::new(VecDeque::with_capacity(MAX_LOG_ENTRIES)));
        let print_logs = Rc::clone(&logs);
        install_print_functions(
            &lua,
            Rc::new(move |level, message| {
                let level = match level {
                    CompatibilityPrintLevel::Trace => ScriptLogLevel::Trace,
                    CompatibilityPrintLevel::Debug => ScriptLogLevel::Debug,
                    CompatibilityPrintLevel::Info => ScriptLogLevel::Info,
                    CompatibilityPrintLevel::Warning => ScriptLogLevel::Warning,
                    CompatibilityPrintLevel::Error => ScriptLogLevel::Error,
                };
                let mut logs = print_logs.borrow_mut();
                if logs.len() == MAX_LOG_ENTRIES {
                    logs.pop_front();
                }
                logs.push_back(ScriptLogEntry { level, message });
            }),
        )?;
        let run_sandboxed = prepare_compatibility_environment(&lua)?;
        let listening = Rc::new(Cell::new(false));
        let mark_listening_state = Rc::clone(&listening);
        let callbacks = ScriptCallbacks::new();
        install_control_api(
            &lua,
            &run_sandboxed,
            bridge,
            &callbacks,
            Rc::new(move || mark_listening_state.set(true)),
        )?;
        install_require(&lua, &run_sandboxed)?;
        Ok(Self {
            lua,
            run_sandboxed,
            logs,
            listening,
            callbacks,
        })
    }

    pub fn lua(&self) -> &Lua {
        &self.lua
    }

    pub fn execute(&self, name: &str, source: &str) -> anyhow::Result<()> {
        self.run_sandboxed
            .call::<()>(source)
            .map_err(|error| anyhow!("could not execute Lua source {name}: {error}"))
    }

    pub fn evaluate_integer(&self, source: &str) -> anyhow::Result<i64> {
        self.run_sandboxed
            .call(source)
            .map_err(|error| anyhow!("could not evaluate Lua integer expression: {error}"))
    }

    pub fn check_syntax(&self, name: &str, source: &str) -> anyhow::Result<()> {
        self.lua
            .load(source)
            .set_name(name)
            .into_function()
            .map_err(|error| anyhow!("could not compile Lua source {name}: {error}"))?;
        Ok(())
    }

    pub fn mark_listening(&self) {
        self.listening.set(true);
    }

    pub fn is_listening(&self) -> bool {
        self.listening.get() || self.callbacks.has_activity()
    }

    pub fn logs(&self) -> Vec<ScriptLogEntry> {
        self.logs.borrow().iter().cloned().collect()
    }

    fn dispatch_loop_event(&self, event: &ScriptLoopEvent) -> Vec<String> {
        self.callbacks.dispatch_loop_event(&self.lua, event)
    }

    fn dispatch_global_event(&self) -> Vec<String> {
        self.callbacks.dispatch_global_event(&self.lua)
    }

    fn dispatch_key_event(&self, event: ScriptKeyEvent) -> Vec<String> {
        self.callbacks.dispatch_key_event(&self.lua, event)
    }

    fn advance_timers(&self, elapsed: std::time::Duration) -> Vec<String> {
        let errors = self.callbacks.advance_timers(elapsed);
        self.listening.set(false);
        errors
    }

    fn has_midi_rules(&self) -> bool {
        self.callbacks.has_midi_rules()
    }

    fn advance_midi(
        &self,
        service: &mut dyn MidiControlService,
        endpoints: &[MidiEndpoint],
        elapsed: std::time::Duration,
    ) -> Vec<String> {
        self.callbacks
            .advance_midi(&self.lua, service, endpoints, elapsed)
    }

    fn disconnect_midi(&self, service: &mut dyn MidiControlService) {
        self.callbacks.disconnect_midi(service);
    }

    fn midi_diagnostics(&self) -> MidiRuntimeDiagnostics {
        self.callbacks.midi_diagnostics()
    }
}

pub fn install_print_functions(
    lua: &Lua,
    handler: Rc<dyn Fn(CompatibilityPrintLevel, String)>,
) -> anyhow::Result<()> {
    for (name, level) in [
        ("__shoop_print", CompatibilityPrintLevel::Info),
        ("__shoop_print_trace", CompatibilityPrintLevel::Trace),
        ("__shoop_print_debug", CompatibilityPrintLevel::Debug),
        ("__shoop_print_info", CompatibilityPrintLevel::Info),
        ("__shoop_print_warning", CompatibilityPrintLevel::Warning),
        ("__shoop_print_error", CompatibilityPrintLevel::Error),
    ] {
        let handler = Rc::clone(&handler);
        let function = lua
            .create_function(move |_, message: String| {
                handler(level, message);
                Ok(())
            })
            .map_err(|error| anyhow!("could not create Lua print function {name}: {error}"))?;
        lua.globals()
            .set(name, function)
            .map_err(|error| anyhow!("could not install Lua print function {name}: {error}"))?;
    }
    Ok(())
}

pub fn prepare_compatibility_environment(lua: &Lua) -> anyhow::Result<Function> {
    lua.load(SANDBOX_SOURCE)
        .set_name("sandbox.lua")
        .exec()
        .map_err(|error| anyhow!("could not prepare Lua compatibility environment: {error}"))?;
    lua.globals()
        .get("__shoop_run_sandboxed")
        .map_err(|error| anyhow!("could not get Lua compatibility runner: {error}"))
}

pub fn install_compatibility_value(
    run_sandboxed: &Function,
    name: &str,
    value: impl mlua::IntoLua,
) -> anyhow::Result<()> {
    let registrar: Function = run_sandboxed
        .call(format!("return function(value) {name} = value end"))
        .map_err(|error| anyhow!("could not create Lua {name} registrar: {error}"))?;
    registrar
        .call::<()>(value)
        .map_err(|error| anyhow!("could not install Lua value {name}: {error}"))
}

fn install_require(lua: &Lua, run_sandboxed: &Function) -> anyhow::Result<()> {
    let libraries: HashMap<String, String> = BUILTIN_LIBRARIES
        .iter()
        .map(|(name, source)| ((*name).to_owned(), (*source).to_owned()))
        .collect();
    let runner = run_sandboxed.clone();
    let require = lua
        .create_function(move |_, name: String| {
            let Some(source) = libraries.get(&name) else {
                return Err(mlua::Error::runtime(format!(
                    "cannot require unloaded library: {name}"
                )));
            };
            runner.call::<Value>(source.as_str())
        })
        .map_err(|error| anyhow!("could not create Lua require function: {error}"))?;
    install_compatibility_value(run_sandboxed, "require", require)?;
    Ok(())
}

struct ScriptRecord {
    id: ScriptId,
    name: String,
    source: String,
    kind: ScriptKind,
    enabled: bool,
    lifecycle: ScriptLifecycle,
    documentation: Option<String>,
    latest_error: Option<String>,
    runtime: Option<LuaRuntime>,
}

pub struct ScriptManager {
    next_id: u64,
    scripts: BTreeMap<ScriptId, ScriptRecord>,
    control: SharedControlBridge,
    midi: Box<dyn MidiControlService>,
    midi_endpoints: Vec<MidiEndpoint>,
    midi_refresh_remaining: std::time::Duration,
    midi_initialized: bool,
}

impl ScriptManager {
    pub fn new() -> Self {
        Self::new_with_midi(Box::<NullMidiService>::default())
    }

    pub fn new_with_midi(midi: Box<dyn MidiControlService>) -> Self {
        Self {
            next_id: 1,
            scripts: BTreeMap::new(),
            control: Rc::new(RefCell::new(ControlBridge::default())),
            midi,
            midi_endpoints: Vec::new(),
            midi_refresh_remaining: std::time::Duration::ZERO,
            midi_initialized: false,
        }
    }

    pub fn set_control_snapshot(&mut self, snapshot: ControlSnapshot) {
        self.control.borrow_mut().snapshot = snapshot;
    }

    pub fn take_control_operations(&mut self) -> Vec<ControlOperation> {
        std::mem::take(&mut self.control.borrow_mut().operations)
    }

    pub fn dispatch_loop_event(&mut self, event: &ScriptLoopEvent) {
        for record in self.scripts.values_mut() {
            let errors = record
                .runtime
                .as_ref()
                .map(|runtime| runtime.dispatch_loop_event(event))
                .unwrap_or_default();
            record_callback_errors(record, errors);
        }
    }

    pub fn dispatch_global_event(&mut self) {
        for record in self.scripts.values_mut() {
            let errors = record
                .runtime
                .as_ref()
                .map(LuaRuntime::dispatch_global_event)
                .unwrap_or_default();
            record_callback_errors(record, errors);
        }
    }

    pub fn dispatch_key_event(&mut self, event: ScriptKeyEvent) {
        for record in self.scripts.values_mut() {
            let errors = record
                .runtime
                .as_ref()
                .map(|runtime| runtime.dispatch_key_event(event))
                .unwrap_or_default();
            record_callback_errors(record, errors);
        }
    }

    pub fn advance_timers(&mut self, elapsed: std::time::Duration) {
        for record in self.scripts.values_mut() {
            let errors = record
                .runtime
                .as_ref()
                .map(|runtime| runtime.advance_timers(elapsed))
                .unwrap_or_default();
            record_callback_errors(record, errors);
            let finished = record
                .runtime
                .as_ref()
                .is_some_and(|runtime| !runtime.is_listening());
            if finished {
                record.runtime = None;
                record.lifecycle = ScriptLifecycle::Finished;
            }
        }
    }

    pub fn advance_midi(&mut self, elapsed: std::time::Duration) {
        if !self.scripts.values().any(|record| {
            record
                .runtime
                .as_ref()
                .is_some_and(LuaRuntime::has_midi_rules)
        }) {
            return;
        }
        self.midi_refresh_remaining = self.midi_refresh_remaining.saturating_sub(elapsed);
        if !self.midi_initialized || self.midi_refresh_remaining.is_zero() {
            match self.midi.endpoints() {
                Ok(snapshot) => {
                    self.midi_endpoints = snapshot.endpoints;
                    self.midi_initialized = true;
                    self.midi_refresh_remaining = std::time::Duration::from_millis(500);
                }
                Err(error) => {
                    for record in self.scripts.values_mut() {
                        if record.runtime.is_some() {
                            record.latest_error = Some(error.to_string());
                        }
                    }
                    self.midi_refresh_remaining = std::time::Duration::from_secs(1);
                    return;
                }
            }
        }
        let endpoints = self.midi_endpoints.clone();
        let (scripts, midi) = (&mut self.scripts, &mut self.midi);
        for record in scripts.values_mut() {
            let errors = record
                .runtime
                .as_ref()
                .map(|runtime| runtime.advance_midi(midi.as_mut(), &endpoints, elapsed))
                .unwrap_or_default();
            record_callback_errors(record, errors);
        }
    }

    pub fn add(
        &mut self,
        name: impl Into<String>,
        source: impl Into<String>,
        kind: ScriptKind,
        enabled: bool,
    ) -> anyhow::Result<ScriptId> {
        let name = name.into();
        let source = source.into();
        LuaRuntime::new()?.check_syntax(&name, &source)?;
        let id = ScriptId::from_raw(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.scripts.insert(
            id,
            ScriptRecord {
                id,
                documentation: extract_documentation(&source),
                name,
                source,
                kind,
                enabled,
                lifecycle: ScriptLifecycle::Inactive,
                latest_error: None,
                runtime: None,
            },
        );
        if enabled {
            let _ = self.start(id);
        }
        Ok(id)
    }

    pub fn set_enabled(&mut self, id: ScriptId, enabled: bool) -> anyhow::Result<()> {
        self.require(id)?;
        if enabled {
            self.scripts.get_mut(&id).unwrap().enabled = true;
            self.start(id)
        } else {
            self.stop(id)?;
            self.scripts.get_mut(&id).unwrap().enabled = false;
            Ok(())
        }
    }

    pub fn start(&mut self, id: ScriptId) -> anyhow::Result<()> {
        let record = self.scripts.get_mut(&id).ok_or_else(|| stale_script(id))?;
        if let Some(runtime) = record.runtime.take() {
            runtime.disconnect_midi(self.midi.as_mut());
        }
        record.lifecycle = ScriptLifecycle::Running;
        record.latest_error = None;
        let runtime = LuaRuntime::new_with_control(Rc::clone(&self.control))?;
        match runtime.execute(&record.name, &record.source) {
            Ok(()) => {
                if runtime.is_listening() {
                    record.lifecycle = ScriptLifecycle::Listening;
                    record.runtime = Some(runtime);
                } else {
                    record.lifecycle = ScriptLifecycle::Finished;
                }
                Ok(())
            }
            Err(error) => {
                let message = error.to_string();
                record.lifecycle = ScriptLifecycle::Error;
                record.latest_error = Some(message.clone());
                Err(anyhow!(message))
            }
        }
    }

    pub fn stop(&mut self, id: ScriptId) -> anyhow::Result<()> {
        let record = self.scripts.get_mut(&id).ok_or_else(|| stale_script(id))?;
        if let Some(runtime) = record.runtime.take() {
            runtime.disconnect_midi(self.midi.as_mut());
        }
        record.lifecycle = ScriptLifecycle::Inactive;
        record.latest_error = None;
        Ok(())
    }

    pub fn forget(&mut self, id: ScriptId) -> anyhow::Result<()> {
        let record = self.scripts.get(&id).ok_or_else(|| stale_script(id))?;
        if record.kind != ScriptKind::User {
            bail!("only user scripts can be forgotten")
        }
        self.stop(id)?;
        self.scripts.remove(&id);
        Ok(())
    }

    pub fn states(&self) -> Vec<ScriptState> {
        self.scripts
            .values()
            .map(|record| {
                let midi = record
                    .runtime
                    .as_ref()
                    .map(LuaRuntime::midi_diagnostics)
                    .unwrap_or_default();
                ScriptState {
                    id: record.id,
                    name: record.name.clone(),
                    kind: record.kind,
                    enabled: record.enabled,
                    lifecycle: record.lifecycle,
                    documentation: record.documentation.clone(),
                    latest_error: record.latest_error.clone(),
                    midi: ScriptMidiDiagnostics {
                        rules: midi.rules,
                        connections: midi.connections,
                        dropped_messages: midi.dropped_messages,
                        errors: midi.errors,
                    },
                }
            })
            .collect()
    }

    pub fn logs(&self, id: ScriptId) -> anyhow::Result<Vec<ScriptLogEntry>> {
        let record = self.scripts.get(&id).ok_or_else(|| stale_script(id))?;
        Ok(record
            .runtime
            .as_ref()
            .map(LuaRuntime::logs)
            .unwrap_or_default())
    }

    fn require(&self, id: ScriptId) -> anyhow::Result<&ScriptRecord> {
        self.scripts.get(&id).ok_or_else(|| stale_script(id))
    }
}

impl Drop for ScriptManager {
    fn drop(&mut self) {
        let (scripts, midi) = (&mut self.scripts, &mut self.midi);
        for record in scripts.values_mut() {
            if let Some(runtime) = record.runtime.take() {
                runtime.disconnect_midi(midi.as_mut());
            }
        }
    }
}

impl Default for ScriptManager {
    fn default() -> Self {
        Self::new()
    }
}

fn record_callback_errors(record: &mut ScriptRecord, errors: Vec<String>) {
    if let Some(error) = errors.last() {
        record.latest_error = Some(error.clone());
    }
}

fn stale_script(id: ScriptId) -> anyhow::Error {
    anyhow!("unknown script id {id}")
}

pub fn extract_documentation(source: &str) -> Option<String> {
    let mut lines = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        if let Some(comment) = trimmed.strip_prefix("--") {
            lines.push(comment.strip_prefix(' ').unwrap_or(comment).to_owned());
        } else if trimmed.is_empty() {
            continue;
        } else {
            break;
        }
    }
    (!lines.is_empty()).then(|| format!("{}\n", lines.join("\n")))
}

#[cfg(test)]
mod tests {
    use shoop_app_api::{LoopId, LoopMode, TrackId};

    use super::*;

    #[test]
    fn runtime_is_constructed_and_used_on_its_actor_thread() {
        let value = std::thread::spawn(|| {
            let runtime = LuaRuntime::new().unwrap();
            runtime.evaluate_integer("return 20 + 22").unwrap()
        })
        .join()
        .unwrap();

        assert_eq!(value, 42);
    }

    #[test]
    fn production_lua_sources_are_embedded_and_syntactically_valid() {
        let runtime = LuaRuntime::new().unwrap();
        runtime
            .check_syntax("keyboard.lua", KEYBOARD_SCRIPT)
            .unwrap();
        runtime
            .check_syntax("akai_apc_mini_mk1.lua", AKAI_APC_MINI_MK1_SCRIPT)
            .unwrap();
        for (name, source) in BUILTIN_LIBRARIES {
            runtime.check_syntax(name, source).unwrap();
        }
    }

    #[test]
    fn compatibility_require_and_print_are_isolated_per_runtime() {
        let first = LuaRuntime::new().unwrap();
        first
            .execute(
                "first",
                "local midi = require('shoop_midi'); print_debug('first'); if midi.NoteOn ~= 0x90 then error('bad module') end",
            )
            .unwrap();
        let second = LuaRuntime::new().unwrap();
        second.execute("second", "print_error('second')").unwrap();
        assert_eq!(
            first.logs(),
            vec![ScriptLogEntry {
                level: ScriptLogLevel::Debug,
                message: "first".to_owned(),
            }]
        );
        assert_eq!(second.logs()[0].message, "second");
    }

    #[test]
    fn complete_control_surface_is_installed_with_legacy_constants() {
        let runtime = LuaRuntime::new().unwrap();
        let module: mlua::Table = runtime
            .run_sandboxed
            .call("return require('shoop_control')")
            .unwrap();
        for name in CONTROL_FUNCTION_NAMES {
            assert!(
                module.get::<mlua::Function>(*name).is_ok(),
                "missing {name}"
            );
        }
        let constants: mlua::Table = module.get("constants").unwrap();
        assert_eq!(constants.get::<i64>("Key_Up").unwrap(), 16_777_235);
        assert_eq!(
            constants.get::<i64>("KeyModifier_ControlModifier").unwrap(),
            67_108_864
        );
        assert_eq!(constants.get::<i64>("LoopMode_Playing").unwrap(), 2);
    }

    #[test]
    fn control_queries_and_mutations_have_ordered_read_your_writes_behavior() {
        let first_loop = LoopId::from_raw(10);
        let second_loop = LoopId::from_raw(11);
        let bridge = Rc::new(RefCell::new(ControlBridge {
            snapshot: ControlSnapshot {
                loops: vec![
                    ControlLoop {
                        id: first_loop,
                        coords: [0, 0],
                        mode: LoopMode::Stopped,
                        next_mode: None,
                        next_mode_delay: None,
                        length: 0,
                        gain: 1.0,
                        balance: 0.0,
                        selected: false,
                        targeted: false,
                    },
                    ControlLoop {
                        id: second_loop,
                        coords: [0, 1],
                        mode: LoopMode::Playing,
                        next_mode: None,
                        next_mode_delay: None,
                        length: 480,
                        gain: 1.0,
                        balance: 0.0,
                        selected: false,
                        targeted: false,
                    },
                ],
                tracks: vec![ControlTrack {
                    id: TrackId::from_raw(2),
                    index: 0,
                    output_gain_db: 0.0,
                    output_balance: 0.0,
                    output_muted: false,
                    input_gain_db: 0.0,
                    input_muted: false,
                }],
                ..Default::default()
            },
            operations: Vec::new(),
        }));
        let runtime = LuaRuntime::new_with_control(Rc::clone(&bridge)).unwrap();
        runtime
            .execute(
                "control",
                r#"
local c = require('shoop_control')
if c.loop_count({{0,0},{0,1}}) ~= 2 then error('count') end
if c.loop_get_mode({0,1})[1] ~= c.constants.LoopMode_Playing then error('mode') end
c.loop_select({{0,0},{0,1}}, true)
if #c.loop_get_which_selected() ~= 2 then error('selection') end
c.loop_set_gain_fader({0,0}, 0.5)
c.loop_trigger({0,0}, c.constants.LoopMode_Recording)
if c.loop_get_mode({0,0})[1] ~= c.constants.LoopMode_Recording then error('trigger') end
c.track_set_muted(0, true)
if not c.track_get_muted(0)[1] then error('track mute') end
c.set_solo(true)
if not c.get_solo() then error('solo') end
"#,
            )
            .unwrap();
        let bridge = bridge.borrow();
        assert_eq!(bridge.snapshot.loops[0].mode, LoopMode::Recording);
        assert!(bridge.snapshot.tracks[0].output_muted);
        assert!(bridge.snapshot.solo);
        assert_eq!(
            bridge.operations,
            vec![
                ControlOperation::SetLoopSelection {
                    loops: vec![first_loop, second_loop],
                    selected: true,
                    clear_others: true,
                },
                ControlOperation::SetLoopGain {
                    loops: vec![first_loop],
                    gain: control::fader_to_gain(0.5),
                },
                ControlOperation::Trigger {
                    loops: vec![first_loop],
                    mode: LoopMode::Recording,
                },
                ControlOperation::SetTrackMuted {
                    tracks: vec![TrackId::from_raw(2)],
                    muted: true,
                },
                ControlOperation::SetSolo(true),
            ]
        );
    }

    #[test]
    fn manager_tracks_lifecycle_errors_restart_and_teardown() {
        let mut manager = ScriptManager::new();
        let finished = manager
            .add("finished", "print('done')", ScriptKind::User, true)
            .unwrap();
        assert_eq!(manager.states()[0].lifecycle, ScriptLifecycle::Finished);
        manager.stop(finished).unwrap();
        assert_eq!(manager.states()[0].lifecycle, ScriptLifecycle::Inactive);
        manager.start(finished).unwrap();
        assert_eq!(manager.states()[0].lifecycle, ScriptLifecycle::Finished);

        let same_name = manager
            .add("finished", "return", ScriptKind::User, false)
            .unwrap();
        assert_ne!(finished, same_name);

        let failed = manager
            .add("failed", "error('broken')", ScriptKind::User, true)
            .unwrap();
        let failed_state = manager
            .states()
            .into_iter()
            .find(|state| state.id == failed)
            .unwrap();
        assert_eq!(failed_state.lifecycle, ScriptLifecycle::Error);
        assert!(failed_state.latest_error.unwrap().contains("broken"));
        assert_eq!(
            manager
                .states()
                .into_iter()
                .find(|state| state.id == finished)
                .unwrap()
                .lifecycle,
            ScriptLifecycle::Finished
        );
        manager.forget(failed).unwrap();
        assert_eq!(manager.states().len(), 2);
    }

    #[test]
    fn callbacks_and_monotonic_timers_are_ordered_and_script_owned() {
        let mut manager = ScriptManager::new();
        let id = manager
            .add(
                "listener",
                r#"
local c = require('shoop_control')
c.register_loop_event_cb(function(event)
    print_info('loop:' .. event.type .. ':' .. event.coords[1] .. ':' .. event.length)
end)
c.register_global_event_cb(function(event) print_info('global:' .. event.type) end)
c.register_keyboard_event_cb(function(event) print_info('key:' .. event.type .. ':' .. event.key) end)
c.register_one_shot_timer_cb(10, function() print_info('timer') end)
"#,
                ScriptKind::User,
                true,
            )
            .unwrap();
        assert_eq!(manager.states()[0].lifecycle, ScriptLifecycle::Listening);
        manager.dispatch_loop_event(&ScriptLoopEvent {
            coords: [2, 3],
            event_type: 1,
            mode: LoopMode::Playing,
            length: 480,
            selected: true,
            targeted: false,
        });
        manager.dispatch_global_event();
        manager.dispatch_key_event(ScriptKeyEvent {
            event_type: 0,
            key: 32,
            modifiers: 0,
        });
        manager.advance_timers(std::time::Duration::from_millis(9));
        assert_eq!(manager.logs(id).unwrap().len(), 3);
        manager.advance_timers(std::time::Duration::from_millis(1));
        assert_eq!(
            manager
                .logs(id)
                .unwrap()
                .into_iter()
                .map(|entry| entry.message)
                .collect::<Vec<_>>(),
            ["loop:1:2:480", "global:0", "key:0:32", "timer"]
        );
        manager.stop(id).unwrap();
        manager.dispatch_global_event();
        assert!(manager.logs(id).unwrap().is_empty());
    }

    #[test]
    fn callback_failure_is_observable_without_stopping_other_scripts() {
        let mut manager = ScriptManager::new();
        let failing = manager
            .add(
                "failing",
                "local c=require('shoop_control'); c.register_global_event_cb(function() error('callback failed') end)",
                ScriptKind::User,
                true,
            )
            .unwrap();
        let healthy = manager
            .add(
                "healthy",
                "local c=require('shoop_control'); c.register_global_event_cb(function() print('healthy') end)",
                ScriptKind::User,
                true,
            )
            .unwrap();
        manager.dispatch_global_event();
        let failing = manager
            .states()
            .into_iter()
            .find(|state| state.id == failing)
            .unwrap();
        assert!(failing.latest_error.unwrap().contains("callback failed"));
        assert_eq!(manager.logs(healthy).unwrap()[0].message, "healthy");
    }

    #[test]
    fn midi_service_autoconnects_hotplugs_delivers_exact_bytes_and_throttles_output() {
        let (midi, control) = FakeMidiService::new();
        control.set_endpoints(vec![
            MidiEndpoint {
                id: "source-apc".to_owned(),
                name: "APC Mini".to_owned(),
                direction: MidiEndpointDirection::Output,
            },
            MidiEndpoint {
                id: "source-apc-2".to_owned(),
                name: "APC Mini".to_owned(),
                direction: MidiEndpointDirection::Output,
            },
            MidiEndpoint {
                id: "sink-apc".to_owned(),
                name: "APC Mini".to_owned(),
                direction: MidiEndpointDirection::Input,
            },
            MidiEndpoint {
                id: "wrong-anchor".to_owned(),
                name: "prefix APC Mini suffix".to_owned(),
                direction: MidiEndpointDirection::Input,
            },
        ]);
        let mut manager = ScriptManager::new_with_midi(Box::new(midi));
        let id = manager
            .add(
                "midi",
                r#"
local c=require('shoop_control')
c.auto_open_device_specific_midi_control_input('APC Mini', function(message)
    print_info('in:' .. message[1] .. ',' .. message[2] .. ',' .. message[3])
end)
c.auto_open_device_specific_midi_control_output('APC Mini', function(port)
    port.send({1, 2, 255})
end, function(port)
    port.send({3, 4})
end, 100)
"#,
                ScriptKind::User,
                true,
            )
            .unwrap();
        manager.advance_midi(std::time::Duration::from_millis(10));
        assert_eq!(
            control.take_sent(),
            [("sink-apc".to_owned(), vec![1, 2, 255])]
        );
        control.push_input("source-apc", vec![0x90, 0x01, 0x7f]);
        control.push_input("source-apc-2", vec![0x80, 0x02, 0x00]);
        manager.advance_midi(std::time::Duration::from_millis(10));
        assert_eq!(control.take_sent(), [("sink-apc".to_owned(), vec![3, 4])]);
        let logs = manager.logs(id).unwrap();
        assert_eq!(
            logs.iter()
                .map(|entry| entry.message.as_str())
                .collect::<Vec<_>>(),
            ["in:144,1,127", "in:128,2,0"],
            "state: {:?}",
            manager.states()
        );
        let state = manager.states().remove(0);
        assert_eq!(state.midi.rules, 2);
        assert_eq!(state.midi.connections, 3);
        assert_eq!(control.active_connections(), 3);
        manager.advance_midi(std::time::Duration::from_millis(500));
        assert_eq!(control.active_connections(), 3);

        control.set_endpoints(Vec::new());
        manager.advance_midi(std::time::Duration::from_millis(500));
        assert_eq!(manager.states()[0].midi.connections, 0);
        control.set_endpoints(vec![MidiEndpoint {
            id: "sink-apc".to_owned(),
            name: "APC Mini".to_owned(),
            direction: MidiEndpointDirection::Input,
        }]);
        manager.advance_midi(std::time::Duration::from_millis(500));
        assert_eq!(control.take_sent(), [("sink-apc".to_owned(), vec![3, 4])]);
        manager.stop(id).unwrap();
        assert_eq!(manager.states()[0].midi.connections, 0);
        assert_eq!(control.active_connections(), 0);
    }

    #[test]
    fn midi_connection_failures_back_off_and_recover() {
        let (midi, control) = FakeMidiService::new();
        control.set_endpoints(vec![MidiEndpoint {
            id: "source".to_owned(),
            name: "device".to_owned(),
            direction: MidiEndpointDirection::Output,
        }]);
        control.set_fail_connections(true);
        let mut manager = ScriptManager::new_with_midi(Box::new(midi));
        manager
            .add(
                "retry",
                "require('shoop_control').auto_open_device_specific_midi_control_input('device', function() end)",
                ScriptKind::User,
                true,
            )
            .unwrap();
        manager.advance_midi(std::time::Duration::from_millis(1));
        assert_eq!(manager.states()[0].midi.errors, 1);
        control.set_fail_connections(false);
        manager.advance_midi(std::time::Duration::from_millis(249));
        assert_eq!(manager.states()[0].midi.connections, 0);
        manager.advance_midi(std::time::Duration::from_millis(1));
        assert_eq!(manager.states()[0].midi.connections, 1);
    }

    #[test]
    fn invalid_midi_rules_and_messages_are_observable() {
        let (midi, control) = FakeMidiService::new();
        control.set_endpoints(vec![MidiEndpoint {
            id: "sink".to_owned(),
            name: "device".to_owned(),
            direction: MidiEndpointDirection::Input,
        }]);
        let mut manager = ScriptManager::new_with_midi(Box::new(midi));
        assert!(manager
            .add(
                "bad regex",
                "require('shoop_control').auto_open_device_specific_midi_control_input('[', function() end)",
                ScriptKind::User,
                true,
            )
            .is_ok());
        assert_eq!(manager.states()[0].lifecycle, ScriptLifecycle::Error);
        let id = manager
            .add(
                "bad byte",
                "local c=require('shoop_control'); c.auto_open_device_specific_midi_control_output('device', function(port) port.send({256}) end, function() end, 0)",
                ScriptKind::User,
                true,
            )
            .unwrap();
        assert_eq!(
            manager
                .states()
                .into_iter()
                .find(|state| state.id == id)
                .unwrap()
                .lifecycle,
            ScriptLifecycle::Error
        );
        let overflow = manager
            .add(
                "overflow",
                "local c=require('shoop_control'); c.auto_open_device_specific_midi_control_output('', function(port) for i=1,1025 do port.send({1}) end end, function() end, 0)",
                ScriptKind::User,
                true,
            )
            .unwrap();
        manager.advance_midi(std::time::Duration::ZERO);
        assert_eq!(
            manager
                .states()
                .into_iter()
                .find(|state| state.id == overflow)
                .unwrap()
                .midi
                .dropped_messages,
            1
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn native_virtual_midi_round_trip_when_host_facilities_are_available() {
        use midir::os::unix::{VirtualInput, VirtualOutput};

        let token = format!("shoop-script-test-{}", std::process::id());
        let (received_sender, received_receiver) = std::sync::mpsc::channel();
        let input = match midir::MidiInput::new("Shoop test sink") {
            Ok(input) => input,
            Err(error) => {
                eprintln!("SKIP native virtual MIDI test: {error}");
                return;
            }
        };
        let sink = match input.create_virtual(
            &format!("{token}-sink"),
            move |_, message, _| {
                let _ = received_sender.send(message.to_vec());
            },
            (),
        ) {
            Ok(connection) => connection,
            Err(error) => {
                eprintln!("SKIP native virtual MIDI test: {error}");
                return;
            }
        };
        let output = match midir::MidiOutput::new("Shoop test source") {
            Ok(output) => output,
            Err(error) => {
                eprintln!("SKIP native virtual MIDI test: {error}");
                return;
            }
        };
        let mut source = match output.create_virtual(&format!("{token}-source")) {
            Ok(connection) => connection,
            Err(error) => {
                eprintln!("SKIP native virtual MIDI test: {error}");
                return;
            }
        };
        let mut manager = ScriptManager::new_with_midi(Box::new(NativeMidiService::new()));
        let id = manager
            .add(
                "native midi",
                format!(
                    "local c=require('shoop_control'); c.auto_open_device_specific_midi_control_input('.*{token}-source.*', function(message) print_info(message[1]) end); c.auto_open_device_specific_midi_control_output('.*{token}-sink.*', function(port) port.send({{9,8,7}}) end, function() end, 0)"
                ),
                ScriptKind::User,
                true,
            )
            .unwrap();
        manager.advance_midi(std::time::Duration::from_millis(500));
        if manager.states()[0].midi.connections != 2 {
            eprintln!("SKIP native virtual MIDI test: host did not expose virtual endpoints");
            return;
        }
        assert_eq!(
            received_receiver
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap(),
            [9, 8, 7]
        );
        source.send(&[6, 5, 4]).unwrap();
        for _ in 0..20 {
            manager.advance_midi(std::time::Duration::from_millis(10));
            if !manager.logs(id).unwrap().is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(manager.logs(id).unwrap()[0].message, "6");
        drop(sink);
    }

    #[test]
    fn manager_rejects_bad_source_and_protects_non_user_records() {
        let mut manager = ScriptManager::new();
        assert!(manager
            .add("bad", "this is not lua", ScriptKind::User, false)
            .is_err());
        assert!(manager.states().is_empty());
        let bundled = manager
            .add("builtin", "return", ScriptKind::Bundled, false)
            .unwrap();
        assert!(manager.forget(bundled).is_err());
    }

    #[test]
    fn documentation_is_extracted_from_the_leading_comment_block() {
        assert_eq!(
            extract_documentation("-- First\n-- second\n\nprint('x')\n-- later"),
            Some("First\nsecond\n".to_owned())
        );
        assert_eq!(extract_documentation("print('x')"), None);
    }
}
