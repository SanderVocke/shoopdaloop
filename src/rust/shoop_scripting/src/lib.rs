use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fmt::Display;
use std::rc::Rc;

use anyhow::{anyhow, bail};
use omnilua::{Function, Lua, Value};
use shoop_app_api::{
    ephemeral_script_display_name, is_ephemeral_script_version,
    ScriptActivityDiagnostics as ApiScriptActivityDiagnostics, ScriptDialogButtonId,
    ScriptDialogId, ScriptDialogState, ScriptId, ScriptKind, ScriptLifecycle,
    ScriptLogLevel as ApiScriptLogLevel, ScriptLogState, ScriptMidiDiagnostics,
    ScriptMidiEndpointDiagnostics, ScriptMidiRuleDiagnostics, ScriptMidiRuleDirection, ScriptState,
};

mod api_version;
mod control;
mod dialog;
mod key_constants;
mod midi;

use api_version::{install_api_version_announcement, ApiVersionState};
use control::{install_control_api, MidiRuleRuntimeDirection, ScriptCallbacks};
pub use control::{
    ControlBridge, ControlLoop, ControlOperation, ControlSnapshot, ControlTrack,
    MidiRuntimeDiagnostics, ScriptActivityDiagnostics, ScriptKeyEvent, ScriptLoopEvent,
    SharedControlBridge, CONTROL_FUNCTION_NAMES,
};
use dialog::{install_dialog_api, DialogIdSource, DialogRegistry};
use key_constants::{KEY_CONSTANTS, MODIFIER_CONSTANTS};
#[cfg(not(target_arch = "wasm32"))]
pub use midi::NativeMidiService;
pub use midi::{
    midi_endpoint_host_id, FakeMidiControl, FakeMidiService, MidiConnectionId, MidiControlService,
    MidiEndpoint, MidiEndpointDirection, MidiEndpointSnapshot, NullMidiService,
    MAX_MIDI_MESSAGE_BYTES, MIDI_QUEUE_CAPACITY,
};

pub const KEYBOARD_SCRIPT: &str = include_str!("../../../lua/builtins/keyboard.lua");
pub const AKAI_APC_MINI_MK1_SCRIPT: &str =
    include_str!("../../../lua/builtins/akai_apc_mini_mk1.lua");
pub const DIALOG_EXAMPLE_SCRIPT: &str = include_str!("../../../lua/examples/dialogs.lua");
const SANDBOX_SOURCE: &str = include_str!("../../../lua/system/sandbox.lua");
const MAX_LOG_ENTRIES: usize = 100;

pub const BUILTIN_LIBRARIES: &[(&str, &str)] = &[
    (
        "shoop_control",
        include_str!("../../../lua/lib/shoop_control.lua"),
    ),
    (
        "shoop_dialog",
        include_str!("../../../lua/lib/shoop_dialog.lua"),
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
pub struct SessionScriptSource {
    pub document_id: u64,
    pub name: String,
    pub source: String,
    pub enabled: bool,
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
    api_version: Rc<ApiVersionState>,
    dialogs: Rc<DialogRegistry>,
}

impl LuaRuntime {
    pub fn new() -> anyhow::Result<Self> {
        Self::new_with_services(
            Rc::new(RefCell::new(ControlBridge::default())),
            Rc::new(DialogIdSource::default()),
        )
    }

    pub fn new_with_control(bridge: SharedControlBridge) -> anyhow::Result<Self> {
        Self::new_with_services(bridge, Rc::new(DialogIdSource::default()))
    }

    fn new_with_services(
        bridge: SharedControlBridge,
        dialog_ids: Rc<DialogIdSource>,
    ) -> anyhow::Result<Self> {
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
        let api_version = Rc::new(ApiVersionState::default());
        install_api_version_announcement(&lua, &run_sandboxed, Rc::clone(&api_version))?;
        let listening = Rc::new(Cell::new(false));
        let mark_listening_state = Rc::clone(&listening);
        let callbacks = ScriptCallbacks::new();
        let mark_listening: Rc<dyn Fn()> = Rc::new(move || mark_listening_state.set(true));
        install_control_api(
            &lua,
            &run_sandboxed,
            bridge,
            &callbacks,
            Rc::clone(&mark_listening),
            Rc::clone(&api_version),
        )?;
        let dialogs = Rc::new(DialogRegistry::default());
        install_dialog_api(
            &lua,
            &run_sandboxed,
            Rc::clone(&api_version),
            dialog_ids,
            Rc::clone(&dialogs),
            mark_listening,
        )?;
        install_require(&lua, &run_sandboxed, Rc::clone(&api_version))?;
        Ok(Self {
            lua,
            run_sandboxed,
            logs,
            listening,
            callbacks,
            api_version,
            dialogs,
        })
    }

    pub fn lua(&self) -> &Lua {
        &self.lua
    }

    pub fn execute(&self, name: &str, source: &str) -> anyhow::Result<()> {
        self.run_sandboxed
            .call::<_, ()>(source)
            .map_err(|error| anyhow!("could not execute Lua source {name}: {error}"))?;
        self.api_version
            .require_announced()
            .map_err(|error| anyhow!("could not execute Lua source {name}: {error}"))?;
        Ok(())
    }

    #[cfg(test)]
    fn execute_announced(&self, name: &str, source: &str) -> anyhow::Result<()> {
        self.execute(name, &format!("shoop_announce_api_version(1, 0)\n{source}"))
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
        self.listening.get() || self.callbacks.has_activity() || self.dialogs.has_dialogs()
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

    fn activity_diagnostics(&self) -> ScriptActivityDiagnostics {
        self.callbacks.activity_diagnostics()
    }

    fn midi_diagnostics(&self) -> MidiRuntimeDiagnostics {
        self.callbacks.midi_diagnostics()
    }

    fn dialog_states(&self, script_id: ScriptId, script_name: &str) -> Vec<ScriptDialogState> {
        self.dialogs.states(script_id, script_name)
    }

    fn invoke_dialog_button(
        &self,
        dialog_id: ScriptDialogId,
        button_id: ScriptDialogButtonId,
    ) -> anyhow::Result<()> {
        self.dialogs.invoke(dialog_id, button_id)
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
    value: impl omnilua::IntoLua,
) -> anyhow::Result<()> {
    let registrar: Function = run_sandboxed
        .call(format!("return function(value) {name} = value end"))
        .map_err(|error| anyhow!("could not create Lua {name} registrar: {error}"))?;
    registrar
        .call::<_, ()>(value)
        .map_err(|error| anyhow!("could not install Lua value {name}: {error}"))
}

fn runtime_error(message: impl Display) -> omnilua::Error {
    omnilua::LuaError::runtime(format_args!("{message}")).into()
}

fn install_require(
    lua: &Lua,
    run_sandboxed: &Function,
    api_version: Rc<ApiVersionState>,
) -> anyhow::Result<()> {
    let libraries: HashMap<String, String> = BUILTIN_LIBRARIES
        .iter()
        .map(|(name, source)| ((*name).to_owned(), (*source).to_owned()))
        .collect();
    let runner = run_sandboxed.clone();
    let require = lua
        .create_function(move |_, name: String| {
            api_version.require_announced()?;
            let Some(source) = libraries.get(&name) else {
                return Err(runtime_error(format!(
                    "cannot require unloaded library: {name}"
                )));
            };
            runner.call::<_, Value>(source.as_str())
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
    session_document_id: Option<u64>,
    archived_logs: Vec<ScriptLogEntry>,
    runtime: Option<LuaRuntime>,
}

pub struct ScriptManager {
    next_id: u64,
    scripts: BTreeMap<ScriptId, ScriptRecord>,
    dialog_ids: Rc<DialogIdSource>,
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
            dialog_ids: Rc::new(DialogIdSource::default()),
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
                if let Some(runtime) = record.runtime.take() {
                    record.archived_logs = runtime.logs();
                }
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
                session_document_id: None,
                archived_logs: Vec::new(),
                runtime: None,
            },
        );
        if enabled {
            let _ = self.start(id);
        }
        Ok(id)
    }

    pub fn add_ephemeral(
        &mut self,
        source_name: impl Into<String>,
        source: impl Into<String>,
    ) -> anyhow::Result<ScriptId> {
        let source_name = source_name.into();
        let source = source.into();
        LuaRuntime::new()?.check_syntax(&source_name, &source)?;
        let display_name = ephemeral_script_display_name(
            &source_name,
            self.scripts.values().map(|record| record.name.as_str()),
        );
        let active_versions = self
            .scripts
            .values()
            .filter(|record| {
                is_ephemeral_script_version(&record.name, &source_name)
                    && matches!(
                        record.lifecycle,
                        ScriptLifecycle::Running | ScriptLifecycle::Listening
                    )
            })
            .map(|record| record.id)
            .collect::<Vec<_>>();
        for id in active_versions {
            self.stop(id)?;
        }
        self.add(display_name, source, ScriptKind::Ephemeral, true)
    }

    #[cfg(test)]
    fn add_announced(
        &mut self,
        name: impl Into<String>,
        source: impl Into<String>,
        kind: ScriptKind,
        enabled: bool,
    ) -> anyhow::Result<ScriptId> {
        self.add(
            name,
            format!("shoop_announce_api_version(1, 0)\n{}", source.into()),
            kind,
            enabled,
        )
    }

    pub fn validate_session_scripts(scripts: &[SessionScriptSource]) -> anyhow::Result<()> {
        let runtime = LuaRuntime::new()?;
        let mut ids = std::collections::BTreeSet::new();
        for script in scripts {
            if !ids.insert(script.document_id) {
                bail!("duplicate session script id {}", script.document_id);
            }
            runtime.check_syntax(&script.name, &script.source)?;
        }
        Ok(())
    }

    pub fn replace_session_scripts(
        &mut self,
        scripts: &[SessionScriptSource],
    ) -> anyhow::Result<()> {
        Self::validate_session_scripts(scripts)?;
        let old_ids = self
            .scripts
            .values()
            .filter(|record| record.kind == ScriptKind::Session)
            .map(|record| record.id)
            .collect::<Vec<_>>();
        for id in old_ids {
            self.stop(id)?;
            self.scripts.remove(&id);
        }
        for script in scripts {
            let id = self.add(
                script.name.clone(),
                script.source.clone(),
                ScriptKind::Session,
                script.enabled,
            )?;
            self.scripts.get_mut(&id).unwrap().session_document_id = Some(script.document_id);
        }
        Ok(())
    }

    pub fn session_scripts(&self) -> Vec<SessionScriptSource> {
        self.scripts
            .values()
            .filter(|record| record.kind == ScriptKind::Session)
            .map(|record| SessionScriptSource {
                document_id: record.session_document_id.unwrap_or(record.id.raw()),
                name: record.name.clone(),
                source: record.source.clone(),
                enabled: record.enabled,
            })
            .collect()
    }

    pub fn replace_user_source(&mut self, id: ScriptId, source: String) -> anyhow::Result<()> {
        let record = self.scripts.get(&id).ok_or_else(|| stale_script(id))?;
        if record.kind != ScriptKind::User {
            bail!("only user script source can be reloaded")
        }
        LuaRuntime::new()?.check_syntax(&record.name, &source)?;
        let enabled = record.enabled;
        let record = self.scripts.get_mut(&id).unwrap();
        record.documentation = extract_documentation(&source);
        record.source = source;
        if enabled {
            self.start(id)
        } else {
            Ok(())
        }
    }

    #[cfg(test)]
    fn replace_user_source_announced(
        &mut self,
        id: ScriptId,
        source: String,
    ) -> anyhow::Result<()> {
        self.replace_user_source(id, format!("shoop_announce_api_version(1, 0)\n{source}"))
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
        record.archived_logs.clear();
        let runtime =
            LuaRuntime::new_with_services(Rc::clone(&self.control), Rc::clone(&self.dialog_ids))?;
        match runtime.execute(&record.name, &record.source) {
            Ok(()) => {
                if runtime.is_listening() {
                    record.lifecycle = ScriptLifecycle::Listening;
                    record.runtime = Some(runtime);
                } else {
                    record.archived_logs = runtime.logs();
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
            record.archived_logs = runtime.logs();
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
                let activity = record
                    .runtime
                    .as_ref()
                    .map(LuaRuntime::activity_diagnostics)
                    .unwrap_or_default();
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
                    activity: ApiScriptActivityDiagnostics {
                        loop_callbacks: activity.loop_callbacks,
                        global_callbacks: activity.global_callbacks,
                        keyboard_callbacks: activity.keyboard_callbacks,
                        timers: activity.timers,
                    },
                    midi: ScriptMidiDiagnostics {
                        rules: midi.rules,
                        connections: midi.connections,
                        dropped_messages: midi.dropped_messages,
                        errors: midi.errors,
                        rule_states: midi
                            .rule_states
                            .into_iter()
                            .map(|rule| ScriptMidiRuleDiagnostics {
                                direction: match rule.direction {
                                    MidiRuleRuntimeDirection::Input => {
                                        ScriptMidiRuleDirection::Input
                                    }
                                    MidiRuleRuntimeDirection::Output => {
                                        ScriptMidiRuleDirection::Output
                                    }
                                },
                                pattern: rule.pattern,
                                matched_endpoints: rule.matched_endpoints.into(),
                                connected_endpoints: rule.connected_endpoints.into(),
                                endpoints: rule
                                    .endpoints
                                    .into_iter()
                                    .map(|endpoint| ScriptMidiEndpointDiagnostics {
                                        id: endpoint.id,
                                        name: endpoint.name,
                                        connected: endpoint.connected,
                                    })
                                    .collect::<Vec<_>>()
                                    .into(),
                                latest_error: rule.latest_error,
                            })
                            .collect::<Vec<_>>()
                            .into(),
                    },
                    logs: record
                        .runtime
                        .as_ref()
                        .map(LuaRuntime::logs)
                        .unwrap_or_else(|| record.archived_logs.clone())
                        .into_iter()
                        .map(|entry| ScriptLogState {
                            level: match entry.level {
                                ScriptLogLevel::Trace => ApiScriptLogLevel::Trace,
                                ScriptLogLevel::Debug => ApiScriptLogLevel::Debug,
                                ScriptLogLevel::Info => ApiScriptLogLevel::Info,
                                ScriptLogLevel::Warning => ApiScriptLogLevel::Warning,
                                ScriptLogLevel::Error => ApiScriptLogLevel::Error,
                            },
                            message: entry.message,
                        })
                        .collect::<Vec<_>>()
                        .into(),
                }
            })
            .collect()
    }

    pub fn dialogs(&self) -> Vec<ScriptDialogState> {
        self.scripts
            .values()
            .flat_map(|record| {
                record
                    .runtime
                    .as_ref()
                    .map(|runtime| runtime.dialog_states(record.id, &record.name))
                    .unwrap_or_default()
            })
            .collect()
    }

    pub fn invoke_dialog_button(
        &mut self,
        script_id: ScriptId,
        dialog_id: ScriptDialogId,
        button_id: ScriptDialogButtonId,
    ) -> anyhow::Result<()> {
        let record = self
            .scripts
            .get_mut(&script_id)
            .ok_or_else(|| stale_script(script_id))?;
        let runtime = record
            .runtime
            .as_ref()
            .ok_or_else(|| anyhow!("script {script_id} is not running"))?;
        match runtime.invoke_dialog_button(dialog_id, button_id) {
            Ok(()) => Ok(()),
            Err(error) => {
                record.latest_error = Some(error.to_string());
                Err(error)
            }
        }
    }

    pub fn logs(&self, id: ScriptId) -> anyhow::Result<Vec<ScriptLogEntry>> {
        let record = self.scripts.get(&id).ok_or_else(|| stale_script(id))?;
        Ok(record
            .runtime
            .as_ref()
            .map(LuaRuntime::logs)
            .unwrap_or_else(|| record.archived_logs.clone()))
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
    use shoop_app_api::{ScriptDialogElement, ScriptDialogKind};

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
        runtime
            .check_syntax("dialogs.lua", DIALOG_EXAMPLE_SCRIPT)
            .unwrap();
        for (name, source) in BUILTIN_LIBRARIES {
            runtime.check_syntax(name, source).unwrap();
        }
    }

    #[test]
    fn compatibility_require_and_print_are_isolated_per_runtime() {
        let first = LuaRuntime::new().unwrap();
        first
            .execute_announced(
                "first",
                "local midi = require('shoop_midi'); print_debug('first'); if midi.NoteOn ~= 0x90 then error('bad module') end",
            )
            .unwrap();
        let second = LuaRuntime::new().unwrap();
        second
            .execute_announced("second", "print_error('second')")
            .unwrap();
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
    fn api_version_announcement_is_mandatory_stable_and_side_effect_free() {
        for (source, expected) in [
            ("return", "must be the first Shoop API call"),
            (
                "shoop_announce_api_version(2, 0)",
                "script requests 2.0, host supports 1.0",
            ),
            (
                "shoop_announce_api_version(0, 0)",
                "script requests 0.0, host supports 1.0",
            ),
            (
                "shoop_announce_api_version(1, 1)",
                "script requests 1.1, host supports 1.0",
            ),
            (
                "shoop_announce_api_version(-1, 0)",
                "major version must be a non-negative integer",
            ),
            (
                "shoop_announce_api_version(1.0, 0)",
                "major version must be a non-negative integer",
            ),
            (
                "shoop_announce_api_version(1)",
                "minor version must be a non-negative integer",
            ),
        ] {
            let runtime = LuaRuntime::new().unwrap();
            let error = runtime.execute("version", source).unwrap_err().to_string();
            assert!(error.contains(expected), "{error:?}");
        }

        let runtime = LuaRuntime::new().unwrap();
        let error = runtime
            .execute(
                "repeated",
                "shoop_announce_api_version(1, 0); shoop_announce_api_version(1, 0)",
            )
            .unwrap_err()
            .to_string();
        assert!(error.contains("may only be called once"));

        let runtime = LuaRuntime::new().unwrap();
        let error = runtime
            .execute(
                "caught rejection",
                "pcall(function() shoop_announce_api_version(2, 0) end); pcall(function() shoop_announce_api_version(1, 0) end)",
            )
            .unwrap_err()
            .to_string();
        assert!(error.contains("announcement was rejected"));

        let bridge = Rc::new(RefCell::new(ControlBridge::default()));
        let runtime = LuaRuntime::new_with_control(Rc::clone(&bridge)).unwrap();
        let error = runtime
            .execute("unannounced", "__shoop_control.set_solo(true)")
            .unwrap_err()
            .to_string();
        assert!(error.contains("must be the first Shoop API call"));
        assert!(!bridge.borrow().snapshot.solo);
        assert!(bridge.borrow().operations.is_empty());
    }

    #[test]
    fn dialogs_preserve_order_styles_opening_callbacks_and_runtime_ownership() {
        let mut manager = ScriptManager::new();
        let id = manager
            .add_announced(
                "owner.lua",
                r#"
local c = require('shoop_control')
local d = require('shoop_dialog')
d.simple('Simple', {
    d.rich_text('first', {strong=true, italics=true, monospace=true, underline=true, strikethrough=true}),
    d.button('No action'),
    d.button('Run', function() c.set_solo(true); d.open('Paged') end),
})
d.paged('Paged', {
    {d.rich_text('page one')},
    {d.rich_text('page two'), d.button('Fail', function() error('dialog callback failed') end)},
})
d.open('Simple')
"#,
                ScriptKind::User,
                true,
            )
            .unwrap();
        assert_eq!(manager.states()[0].lifecycle, ScriptLifecycle::Listening);
        let dialogs = manager.dialogs();
        assert_eq!(
            dialogs
                .iter()
                .map(|dialog| dialog.name.as_str())
                .collect::<Vec<_>>(),
            ["Simple", "Paged"]
        );
        assert_eq!(dialogs[0].owner_script_id, id);
        assert_eq!(dialogs[0].owner_script_name, "owner.lua");
        assert_eq!(dialogs[0].open_request, 1);
        let ScriptDialogKind::Simple(simple) = &dialogs[0].kind else {
            panic!("expected simple dialog");
        };
        let ScriptDialogElement::RichText { text, style } = &simple.elements[0] else {
            panic!("expected rich text");
        };
        assert_eq!(text, "first");
        assert!(style.strong && style.italics && style.monospace);
        assert!(style.underline && style.strikethrough);
        let ScriptDialogElement::Button { id: None, label } = &simple.elements[1] else {
            panic!("expected callback-free button");
        };
        assert_eq!(label, "No action");
        let ScriptDialogElement::Button {
            id: Some(run_button),
            ..
        } = &simple.elements[2]
        else {
            panic!("expected callback button");
        };
        manager
            .invoke_dialog_button(id, dialogs[0].id, *run_button)
            .unwrap();
        assert_eq!(manager.dialogs()[1].open_request, 1);
        assert_eq!(
            manager.take_control_operations(),
            [ControlOperation::SetSolo(true)]
        );

        let ScriptDialogKind::Paged(pages) = &dialogs[1].kind else {
            panic!("expected paged dialog");
        };
        assert_eq!(pages.len(), 2);
        let ScriptDialogElement::Button {
            id: Some(fail_button),
            ..
        } = &pages[1].elements[1]
        else {
            panic!("expected callback button");
        };
        let error = manager
            .invoke_dialog_button(id, dialogs[1].id, *fail_button)
            .unwrap_err();
        assert!(error.to_string().contains("dialog callback failed"));
        assert!(manager.states()[0]
            .latest_error
            .as_deref()
            .unwrap()
            .contains("dialog callback failed"));

        let first_generation = dialogs[0].id;
        manager.start(id).unwrap();
        let restarted = manager.dialogs();
        assert_ne!(restarted[0].id, first_generation);
        assert!(manager
            .invoke_dialog_button(id, first_generation, *run_button)
            .is_err());
        manager.stop(id).unwrap();
        assert!(manager.dialogs().is_empty());
    }

    #[test]
    fn dialog_definition_validation_and_failed_startup_leave_no_runtime_state() {
        for (body, expected) in [
            (
                "local d=require('shoop_dialog'); d.simple('', {d.rich_text('x')})",
                "dialog name must not be empty",
            ),
            (
                "local d=require('shoop_dialog'); d.simple('x', {})",
                "at least one element",
            ),
            (
                "local d=require('shoop_dialog'); d.paged('x', {})",
                "at least one page",
            ),
            (
                "local d=require('shoop_dialog'); d.rich_text('x', {color=true})",
                "unknown dialog rich-text style",
            ),
            (
                "local d=require('shoop_dialog'); d.button('   ')",
                "button label must not be empty",
            ),
            (
                "local d=require('shoop_dialog'); d.open('missing')",
                "unknown script dialog",
            ),
            (
                "local d=require('shoop_dialog'); d.simple('x',{d.rich_text('a')}); d.simple('x',{d.rich_text('b')})",
                "already defined",
            ),
        ] {
            let runtime = LuaRuntime::new().unwrap();
            let error = runtime
                .execute_announced("invalid dialog", body)
                .unwrap_err()
                .to_string();
            assert!(error.contains(expected), "{error:?}");
        }

        let mut manager = ScriptManager::new();
        let id = manager
            .add_announced(
                "partial",
                "local d=require('shoop_dialog'); d.simple('temporary',{d.rich_text('x')}); error('later failure')",
                ScriptKind::User,
                true,
            )
            .unwrap();
        assert!(manager.dialogs().is_empty());
        assert_eq!(manager.states()[0].id, id);
        assert_eq!(manager.states()[0].lifecycle, ScriptLifecycle::Error);
    }

    #[test]
    fn complete_control_surface_is_installed_with_stable_constants() {
        let runtime = LuaRuntime::new().unwrap();
        runtime
            .run_sandboxed
            .call::<_, ()>("shoop_announce_api_version(1, 0)")
            .unwrap();
        let module: omnilua::Table = runtime
            .run_sandboxed
            .call("return require('shoop_control')")
            .unwrap();
        for name in CONTROL_FUNCTION_NAMES {
            assert!(
                module.get::<_, omnilua::Function>(*name).is_ok(),
                "missing {name}"
            );
        }
        let constants: omnilua::Table = module.get("constants").unwrap();
        assert_eq!(constants.get::<_, i64>("Key_Up").unwrap(), 16_777_235);
        assert_eq!(
            constants
                .get::<_, i64>("KeyModifier_ControlModifier")
                .unwrap(),
            67_108_864
        );
        assert_eq!(constants.get::<_, i64>("LoopMode_Playing").unwrap(), 2);
        for &(name, expected) in KEY_CONSTANTS.iter().chain(MODIFIER_CONSTANTS.iter()) {
            assert_eq!(constants.get::<_, i64>(name).unwrap(), expected, "{name}");
        }
        for (name, expected) in [
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
            ("Loop_DontWaitForSync", -1),
            ("Loop_DontAlignToSyncImmediately", -1),
        ] {
            assert_eq!(constants.get::<_, i64>(name).unwrap(), expected, "{name}");
        }
        assert_eq!(
            constants.clone().pairs().unwrap().count(),
            KEY_CONSTANTS.len() + MODIFIER_CONSTANTS.len() + 17
        );
    }

    #[test]
    fn every_control_function_is_invoked_with_retained_shapes_and_selectors() {
        let bridge = Rc::new(RefCell::new(ControlBridge {
            snapshot: ControlSnapshot {
                loops: vec![
                    ControlLoop {
                        id: LoopId::from_raw(1),
                        coords: [-1, 0],
                        mode: LoopMode::Stopped,
                        next_mode: None,
                        next_mode_delay: None,
                        length: 100,
                        gain: 1.0,
                        balance: 0.0,
                        selected: false,
                        targeted: true,
                    },
                    ControlLoop {
                        id: LoopId::from_raw(2),
                        coords: [0, 0],
                        mode: LoopMode::Playing,
                        next_mode: Some(LoopMode::Recording),
                        next_mode_delay: Some(2),
                        length: 200,
                        gain: 0.5,
                        balance: -0.25,
                        selected: true,
                        targeted: false,
                    },
                    ControlLoop {
                        id: LoopId::from_raw(3),
                        coords: [1, 0],
                        mode: LoopMode::Recording,
                        next_mode: None,
                        next_mode_delay: None,
                        length: 300,
                        gain: 2.0,
                        balance: 0.5,
                        selected: false,
                        targeted: false,
                    },
                ],
                tracks: vec![
                    ControlTrack {
                        id: TrackId::from_raw(1),
                        index: -1,
                        output_gain_db: 0.0,
                        output_balance: 0.0,
                        output_muted: false,
                        input_gain_db: 0.0,
                        input_muted: false,
                    },
                    ControlTrack {
                        id: TrackId::from_raw(2),
                        index: 0,
                        output_gain_db: -6.0,
                        output_balance: -0.25,
                        output_muted: true,
                        input_gain_db: 6.0,
                        input_muted: true,
                    },
                    ControlTrack {
                        id: TrackId::from_raw(3),
                        index: 1,
                        output_gain_db: 6.0,
                        output_balance: 0.5,
                        output_muted: false,
                        input_gain_db: -6.0,
                        input_muted: false,
                    },
                ],
                apply_n_cycles: 3,
                solo: false,
                sync_active: true,
                play_after_record: true,
                auto_mute_other_track_inputs: false,
                default_recording_action: shoop_app_api::DefaultRecordingAction::Record,
            },
            operations: Vec::new(),
        }));
        let runtime = LuaRuntime::new_with_control(Rc::clone(&bridge)).unwrap();
        runtime
            .execute_announced(
                "complete control table",
                r#"
local c = require('shoop_control')
for name, fn in pairs(c) do
    if type(fn) == 'function' then
        c[name] = function(...)
            print_info('CALL:' .. name)
            return fn(...)
        end
    end
end
local function eq(actual, expected, label)
    if actual ~= expected then error(label .. ': ' .. tostring(actual)) end
end
local function coords(actual, track, row, label)
    eq(actual[1], track, label .. ' track')
    eq(actual[2], row, label .. ' row')
end

eq(c.loop_count({{-1,0},{0,0},{1,0}}), 3, 'loop_count')
local all = c.loop_get_all(); eq(#all, 3, 'loop_get_all size'); coords(all[1], -1, 0, 'all 1')
local selected = c.loop_get_which_selected(); eq(#selected, 1, 'selected size'); coords(selected[1], 0, 0, 'selected')
coords(c.loop_get_which_targeted(), -1, 0, 'targeted')
coords(c.loop_get_by_mode(c.constants.LoopMode_Playing)[1], 0, 0, 'by mode')
eq(c.loop_get_mode({0,0})[1], c.constants.LoopMode_Playing, 'mode')
eq(c.loop_get_next_mode({0,0})[1], c.constants.LoopMode_Recording, 'next mode')
eq(c.loop_get_next_mode_delay({0,0})[1], 2, 'next delay')
eq(c.loop_get_length({0,0})[1], 200, 'length')
coords(c.loop_get_by_track(1)[1], 1, 0, 'by track')
eq(c.loop_get_gain({0,0})[1], 0.5, 'loop gain')
local loop_fader = c.loop_get_gain_fader({0,0})[1]; if loop_fader <= 0 or loop_fader >= 1 then error('loop fader') end
eq(c.loop_get_balance({0,0})[1], -0.25, 'loop balance')

c.loop_transition({0,0}, c.constants.LoopMode_Replacing, c.constants.Loop_DontWaitForSync, c.constants.Loop_DontAlignToSyncImmediately)
c.loop_trigger({1,0}, c.constants.LoopMode_Playing)
c.loop_trigger_grab({0,0})
c.loop_record_n({0,0}, 4, 2)
c.loop_record_with_targeted({0,0})
c.loop_set_gain({0,0}, -2)
c.loop_set_gain_fader({0,0}, 0.75)
c.loop_set_balance({0,0}, 2)
c.loop_select({1,0}, true)
c.loop_target({0,0})
c.loop_untarget_all()
c.loop_toggle_targeted({1,0})
c.loop_toggle_selected({1,0})
c.loop_clear({0,0})
c.loop_clear_all()
c.loop_adopt_ringbuffers({0,0}, 1, 2, 3, c.constants.LoopMode_Playing)
c.loop_compose_add_to_end({0,0}, {{1,0},{-1,0}}, true)
c.loop_set_repeat_sync({0,0}, true)

local tracks = {-1, 0, 1}
eq(#c.track_get_gain(tracks), 3, 'track gain shape')
eq(#c.track_get_balance(tracks), 3, 'track balance shape')
eq(#c.track_get_gain_fader(tracks), 3, 'track fader shape')
eq(#c.track_get_input_gain(tracks), 3, 'input gain shape')
eq(#c.track_get_input_gain_fader(tracks), 3, 'input fader shape')
eq(c.track_get_muted(0)[1], true, 'track muted')
eq(c.track_get_input_muted(0)[1], true, 'input muted')
c.track_set_muted({0,1}, false)
c.track_set_input_muted({0,1}, false)
c.track_set_gain({0,1}, 0.5)
c.track_set_gain_fader({0,1}, 0.75)
c.track_set_balance({0,1}, 2)
c.track_set_input_gain({0,1}, 2)
c.track_set_input_gain_fader({0,1}, 0.25)

eq(c.get_apply_n_cycles(), 3, 'cycles')
c.set_apply_n_cycles(5)
eq(c.get_solo(), false, 'solo')
c.set_solo(true)
eq(c.get_sync_active(), true, 'sync')
c.set_sync_active(false)
eq(c.get_play_after_record(), true, 'play after record')
c.set_play_after_record(false)
eq(c.get_auto_mute_other_track_inputs(), false, 'auto mute inputs')
c.set_auto_mute_other_track_inputs(true)
eq(c.get_default_recording_action(), 'record', 'record action')
c.set_default_recording_action('grab')

c.register_loop_event_cb(function() end)
c.register_global_event_cb(function() end)
c.register_keyboard_event_cb(function() end)
c.register_one_shot_timer_cb(0, function() end)
c.auto_open_device_specific_midi_control_input('', function() end)
c.auto_open_device_specific_midi_control_output('', function() end, function() end, 0)
"#,
            )
            .unwrap();

        let called = runtime
            .logs()
            .into_iter()
            .filter_map(|entry| entry.message.strip_prefix("CALL:").map(str::to_owned))
            .collect::<std::collections::BTreeSet<_>>();
        let expected = CONTROL_FUNCTION_NAMES
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(called, expected);
        assert_eq!(bridge.borrow().operations.len(), 31);
    }

    #[test]
    fn control_queries_follow_track_and_loop_coordinate_reordering() {
        let bridge = Rc::new(RefCell::new(ControlBridge {
            snapshot: ControlSnapshot {
                loops: vec![
                    ControlLoop {
                        id: LoopId::from_raw(1),
                        coords: [0, 0],
                        mode: LoopMode::Stopped,
                        next_mode: None,
                        next_mode_delay: None,
                        length: 100,
                        gain: 1.0,
                        balance: 0.0,
                        selected: false,
                        targeted: false,
                    },
                    ControlLoop {
                        id: LoopId::from_raw(2),
                        coords: [0, 1],
                        mode: LoopMode::Playing,
                        next_mode: None,
                        next_mode_delay: None,
                        length: 200,
                        gain: 1.0,
                        balance: 0.0,
                        selected: false,
                        targeted: false,
                    },
                ],
                ..Default::default()
            },
            operations: Vec::new(),
        }));
        let runtime = LuaRuntime::new_with_control(Rc::clone(&bridge)).unwrap();
        runtime
            .execute_announced(
                "before reorder",
                "local c=require('shoop_control'); if c.loop_get_mode({0,0})[1] ~= c.constants.LoopMode_Stopped then error('mode before reorder') end",
            )
            .unwrap();
        {
            let mut bridge = bridge.borrow_mut();
            bridge.snapshot.loops[0].coords = [1, 0];
            bridge.snapshot.loops[1].coords = [0, 0];
        }
        runtime
            .execute(
                "after reorder",
                r#"
local c=require('shoop_control')
if c.loop_get_mode({0,0})[1] ~= c.constants.LoopMode_Playing then error('mode after reorder') end
local track_zero = c.loop_get_by_track(0)
if #track_zero ~= 1 or track_zero[1][1] ~= 0 or track_zero[1][2] ~= 0 then error('track zero') end
local track_one = c.loop_get_by_track(1)
if #track_one ~= 1 or track_one[1][1] ~= 1 or track_one[1][2] ~= 0 then error('track one') end
"#,
            )
            .unwrap();
    }

    #[test]
    fn control_argument_validation_is_observable_and_non_mutating() {
        let first_loop = LoopId::from_raw(10);
        let bridge = Rc::new(RefCell::new(ControlBridge {
            snapshot: ControlSnapshot {
                loops: vec![ControlLoop {
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
                }],
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
            .execute_announced(
                "invalid arguments",
                r#"
local c=require('shoop_control')
local function fails(fragment, fn)
    local ok, error = pcall(fn)
    if ok or not tostring(error):find(fragment, 1, true) then
        error('expected failure containing ' .. fragment .. ': ' .. tostring(error))
    end
end
fails('loop coordinate', function() c.loop_count({{0}}) end)
fails('invalid loop mode', function() c.loop_trigger({0,0}, 99) end)
fails('non-negative', function() c.loop_transition({0,0}, c.constants.LoopMode_Playing, -2, -1) end)
fails('track selector', function() c.track_get_gain('bad') end)
fails('2 or 3 arguments', function() c.track_set_input_muted(0) end)
fails('muted must be boolean', function() c.track_set_input_muted(0, 'false') end)
fails('respect_auto_mute must be boolean', function() c.track_set_input_muted(0, false, 1) end)
fails('2 or 3 arguments', function() c.track_set_input_muted(0, false, true, false) end)
fails('timer delay', function() c.register_one_shot_timer_cb(-1, function() end) end)
fails('rate limit', function() c.auto_open_device_specific_midi_control_output('', function() end, function() end, -1) end)
fails('invalid MIDI autoconnect regex', function() c.auto_open_device_specific_midi_control_input('[', function() end) end)
c.loop_trigger({99,99}, c.constants.LoopMode_Playing)
c.track_set_muted(99, true)
"#,
            )
            .unwrap();
        assert_eq!(
            bridge.borrow().operations,
            [
                ControlOperation::Trigger {
                    loops: Vec::new(),
                    mode: LoopMode::Playing,
                },
                ControlOperation::SetTrackMuted {
                    tracks: Vec::new(),
                    muted: true,
                },
            ]
        );
    }

    #[test]
    fn input_mute_control_and_helper_preserve_legacy_and_exclusive_semantics() {
        let sync = TrackId::from_raw(1);
        let first = TrackId::from_raw(2);
        let second = TrackId::from_raw(3);
        let third = TrackId::from_raw(4);
        let track = |id, index, input_muted| ControlTrack {
            id,
            index,
            output_gain_db: 0.0,
            output_balance: 0.0,
            output_muted: false,
            input_gain_db: 0.0,
            input_muted,
        };
        let bridge = Rc::new(RefCell::new(ControlBridge {
            snapshot: ControlSnapshot {
                tracks: vec![
                    track(sync, -1, false),
                    track(first, 0, false),
                    track(second, 1, true),
                    track(third, 2, false),
                ],
                auto_mute_other_track_inputs: true,
                ..Default::default()
            },
            operations: Vec::new(),
        }));
        let runtime = LuaRuntime::new_with_control(Rc::clone(&bridge)).unwrap();
        runtime
            .execute_announced(
                "input mutedness",
                r#"
local c = require('shoop_control')
local h = require('shoop_helpers')
local function states(expected)
    local actual = c.track_get_input_muted({-1, 0, 1, 2})
    for i, value in ipairs(expected) do
        if actual[i] ~= value then error('state ' .. i) end
    end
end
if not c.get_auto_mute_other_track_inputs() then error('global getter') end
h.track_toggle_input_muted(0)
states({false, true, true, false})
h.track_toggle_input_muted({0, 1}, true)
states({true, false, false, true})
h.track_toggle_input_muted({0, 1}, true)
states({true, true, true, true})
c.track_set_input_muted(2, false)
states({true, true, true, false})
c.set_auto_mute_other_track_inputs(false)
if c.get_auto_mute_other_track_inputs() then error('global setter') end
"#,
            )
            .unwrap();
        assert_eq!(
            bridge.borrow().operations,
            [
                ControlOperation::SetTrackInputMuted {
                    tracks: vec![first],
                    muted: true,
                    respect_auto_mute: false,
                },
                ControlOperation::SetTrackInputMuted {
                    tracks: vec![first, second],
                    muted: false,
                    respect_auto_mute: true,
                },
                ControlOperation::SetTrackInputMuted {
                    tracks: vec![first, second],
                    muted: true,
                    respect_auto_mute: true,
                },
                ControlOperation::SetTrackInputMuted {
                    tracks: vec![third],
                    muted: false,
                    respect_auto_mute: false,
                },
                ControlOperation::SetAutoMuteOtherTrackInputs(false),
            ]
        );
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
            .execute_announced(
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
            .add_announced("finished", "print('done')", ScriptKind::User, true)
            .unwrap();
        assert_eq!(manager.states()[0].lifecycle, ScriptLifecycle::Finished);
        manager.stop(finished).unwrap();
        assert_eq!(manager.states()[0].lifecycle, ScriptLifecycle::Inactive);
        manager.start(finished).unwrap();
        assert_eq!(manager.states()[0].lifecycle, ScriptLifecycle::Finished);

        let same_name = manager
            .add_announced("finished", "return", ScriptKind::User, false)
            .unwrap();
        assert_ne!(finished, same_name);

        let failed = manager
            .add_announced("failed", "error('broken')", ScriptKind::User, true)
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
    fn ephemeral_versions_stop_active_names_and_retain_restartable_sources() {
        let mut manager = ScriptManager::new();
        let listener =
            "local c=require('shoop_control'); c.register_global_event_cb(function() end)";
        let builtin = manager
            .add_announced("controller.lua", listener, ScriptKind::Bundled, true)
            .unwrap();
        let second = manager
            .add_ephemeral(
                "controller.lua",
                format!("shoop_announce_api_version(1, 0)\n{listener}"),
            )
            .unwrap();
        let states = manager.states();
        assert_eq!(states[0].id, builtin);
        assert_eq!(states[0].lifecycle, ScriptLifecycle::Inactive);
        assert_eq!(states[1].id, second);
        assert_eq!(states[1].name, "controller.lua (run once 2)");
        assert_eq!(states[1].kind, ScriptKind::Ephemeral);
        assert_eq!(states[1].lifecycle, ScriptLifecycle::Listening);

        assert!(manager
            .add_ephemeral("controller.lua", "function(")
            .is_err());
        assert_eq!(manager.states()[1].lifecycle, ScriptLifecycle::Listening);

        let third = manager
            .add_ephemeral(
                "controller.lua",
                "shoop_announce_api_version(1, 0); print('new')",
            )
            .unwrap();
        let states = manager.states();
        assert_eq!(states[1].lifecycle, ScriptLifecycle::Inactive);
        assert_eq!(states[2].id, third);
        assert_eq!(states[2].name, "controller.lua (run once 3)");
        assert_eq!(states[2].lifecycle, ScriptLifecycle::Finished);
        manager.start(second).unwrap();
        assert_eq!(manager.states()[1].lifecycle, ScriptLifecycle::Listening);
    }

    #[test]
    fn callbacks_and_monotonic_timers_are_ordered_and_script_owned() {
        let mut manager = ScriptManager::new();
        let id = manager
            .add_announced(
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
        assert_eq!(manager.logs(id).unwrap().len(), 4);
    }

    #[test]
    fn callbacks_expose_complete_payloads_and_are_non_reentrant() {
        let mut manager = ScriptManager::new();
        let id = manager
            .add_announced(
                "payloads",
                r#"
local c = require('shoop_control')
local installed_nested = false
c.register_loop_event_cb(function(event)
    print_info(string.format('loop:%d:%d,%d:%d:%d:%s:%s', event.type,
        event.coords[1], event.coords[2], event.mode, event.length,
        tostring(event.selected), tostring(event.targeted)))
    if not installed_nested then
        installed_nested = true
        c.register_loop_event_cb(function(nested)
            print_info('nested:' .. nested.type)
        end)
    end
end)
c.register_global_event_cb(function(event) print_info('global:' .. event.type) end)
c.register_keyboard_event_cb(function(event)
    print_info(string.format('key:%d:%d:%d', event.type, event.key, event.modifiers))
end)
"#,
                ScriptKind::User,
                true,
            )
            .unwrap();

        let event = ScriptLoopEvent {
            coords: [-1, 7],
            event_type: 0,
            mode: LoopMode::RecordingDryIntoWet,
            length: 12_345,
            selected: true,
            targeted: false,
        };
        manager.dispatch_loop_event(&event);
        assert_eq!(manager.logs(id).unwrap().len(), 1);
        for event_type in 1..=4 {
            manager.dispatch_loop_event(&ScriptLoopEvent {
                event_type,
                selected: false,
                targeted: true,
                ..event
            });
        }
        manager.dispatch_global_event();
        manager.dispatch_key_event(ScriptKeyEvent {
            event_type: 1,
            key: 65,
            modifiers: 100_663_296,
        });
        assert_eq!(
            manager
                .logs(id)
                .unwrap()
                .into_iter()
                .map(|entry| entry.message)
                .collect::<Vec<_>>(),
            [
                "loop:0:-1,7:6:12345:true:false",
                "loop:1:-1,7:6:12345:false:true",
                "nested:1",
                "loop:2:-1,7:6:12345:false:true",
                "nested:2",
                "loop:3:-1,7:6:12345:false:true",
                "nested:3",
                "loop:4:-1,7:6:12345:false:true",
                "nested:4",
                "global:0",
                "key:1:65:100663296",
            ]
        );
    }

    #[test]
    fn timers_are_due_ordered_non_reentrant_capped_and_cancelled_on_stop() {
        let mut manager = ScriptManager::new();
        let id = manager
            .add_announced(
                "timers",
                r#"
local c = require('shoop_control')
c.register_one_shot_timer_cb(10, function() print_info('ten') end)
c.register_one_shot_timer_cb(5, function()
    print_info('five-a')
    c.register_one_shot_timer_cb(0, function() print_info('nested-zero') end)
end)
c.register_one_shot_timer_cb(5, function() print_info('five-b') end)
"#,
                ScriptKind::User,
                true,
            )
            .unwrap();
        manager.advance_timers(std::time::Duration::from_millis(5));
        assert_eq!(
            manager
                .logs(id)
                .unwrap()
                .into_iter()
                .map(|entry| entry.message)
                .collect::<Vec<_>>(),
            ["five-a", "five-b"]
        );
        assert_eq!(manager.states()[0].activity.timers, 2);
        manager.advance_timers(std::time::Duration::ZERO);
        assert_eq!(
            manager.logs(id).unwrap().last().unwrap().message,
            "nested-zero"
        );
        manager.advance_timers(std::time::Duration::from_millis(5));
        assert_eq!(manager.logs(id).unwrap().last().unwrap().message, "ten");

        let registrations = (0..=control::MAX_SCRIPT_CALLBACKS_PER_PUMP)
            .map(|_| "c.register_one_shot_timer_cb(0, function() end)")
            .collect::<Vec<_>>()
            .join("\n");
        let capped = manager
            .add_announced(
                "timer cap",
                format!("local c=require('shoop_control')\n{registrations}"),
                ScriptKind::User,
                true,
            )
            .unwrap();
        manager.advance_timers(std::time::Duration::ZERO);
        let capped_state = manager
            .states()
            .into_iter()
            .find(|state| state.id == capped)
            .unwrap();
        assert_eq!(capped_state.activity.timers, 1);
        manager.stop(capped).unwrap();
        assert_eq!(
            manager
                .states()
                .into_iter()
                .find(|state| state.id == capped)
                .unwrap()
                .activity
                .timers,
            0
        );
    }

    #[test]
    fn callback_failure_is_observable_without_stopping_other_scripts() {
        let mut manager = ScriptManager::new();
        let failing = manager
            .add_announced(
                "failing",
                "local c=require('shoop_control'); c.register_global_event_cb(function() error('callback failed') end)",
                ScriptKind::User,
                true,
            )
            .unwrap();
        let healthy = manager
            .add_announced(
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
            .add_announced(
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
        assert_eq!(state.midi.rule_states.len(), 2);
        assert_eq!(
            state.midi.rule_states[0].direction,
            ScriptMidiRuleDirection::Input
        );
        assert_eq!(state.midi.rule_states[0].pattern, "APC Mini");
        assert_eq!(
            state.midi.rule_states[0].connected_endpoints.as_ref(),
            ["APC Mini [source-apc]", "APC Mini [source-apc-2]"]
        );
        assert_eq!(
            state.midi.rule_states[1].direction,
            ScriptMidiRuleDirection::Output
        );
        assert_eq!(
            state.midi.rule_states[1].connected_endpoints.as_ref(),
            ["APC Mini [sink-apc]"]
        );
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
    fn positive_midi_rate_limit_never_catches_up_with_a_same_pump_burst() {
        let (midi, control) = FakeMidiService::new();
        control.set_endpoints(vec![
            MidiEndpoint {
                id: "sink-a".to_owned(),
                name: "device".to_owned(),
                direction: MidiEndpointDirection::Input,
            },
            MidiEndpoint {
                id: "sink-b".to_owned(),
                name: "device".to_owned(),
                direction: MidiEndpointDirection::Input,
            },
        ]);
        let mut manager = ScriptManager::new_with_midi(Box::new(midi));
        manager
            .add_announced(
                "paced broadcast",
                r#"
local c=require('shoop_control')
c.auto_open_device_specific_midi_control_output('device', function(port)
    port.send({1})
    port.send({2})
    port.send({3})
end, function() end, 10)
"#,
                ScriptKind::User,
                true,
            )
            .unwrap();

        manager.advance_midi(std::time::Duration::from_millis(99));
        assert!(control.take_sent().is_empty());
        manager.advance_midi(std::time::Duration::from_millis(1));
        assert_eq!(
            control.take_sent(),
            [
                ("sink-a".to_owned(), vec![1]),
                ("sink-b".to_owned(), vec![1]),
            ]
        );

        // A late pump may send one message, but must not flush the accumulated backlog.
        manager.advance_midi(std::time::Duration::from_secs(1));
        assert_eq!(
            control.take_sent(),
            [
                ("sink-a".to_owned(), vec![2]),
                ("sink-b".to_owned(), vec![2]),
            ]
        );
        manager.advance_midi(std::time::Duration::ZERO);
        assert!(control.take_sent().is_empty());
        manager.advance_midi(std::time::Duration::from_millis(99));
        assert!(control.take_sent().is_empty());
        manager.advance_midi(std::time::Duration::from_millis(1));
        assert_eq!(
            control.take_sent(),
            [
                ("sink-a".to_owned(), vec![3]),
                ("sink-b".to_owned(), vec![3]),
            ]
        );
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
            .add_announced(
                "retry",
                "require('shoop_control').auto_open_device_specific_midi_control_input('device', function() end)",
                ScriptKind::User,
                true,
            )
            .unwrap();
        manager.advance_midi(std::time::Duration::from_millis(1));
        assert_eq!(manager.states()[0].midi.errors, 1);
        let failed_rule = &manager.states()[0].midi.rule_states[0];
        assert_eq!(failed_rule.pattern, "device");
        assert!(failed_rule.connected_endpoints.is_empty());
        assert!(failed_rule
            .latest_error
            .as_deref()
            .is_some_and(|error| error.contains("connection failure")));
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
            .add_announced(
                "bad regex",
                "require('shoop_control').auto_open_device_specific_midi_control_input('[', function() end)",
                ScriptKind::User,
                true,
            )
            .is_ok());
        assert_eq!(manager.states()[0].lifecycle, ScriptLifecycle::Error);
        let id = manager
            .add_announced(
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
            .add_announced(
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
            .add_announced(
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
    fn user_source_reload_is_syntax_checked_and_preserves_identity() {
        let mut manager = ScriptManager::new();
        let id = manager
            .add_announced(
                "user.lua",
                "local c=require('shoop_control'); c.register_global_event_cb(function() print('old') end)",
                ScriptKind::User,
                true,
            )
            .unwrap();
        assert!(manager
            .replace_user_source_announced(id, "function(".to_owned())
            .is_err());
        manager.dispatch_global_event();
        assert_eq!(manager.logs(id).unwrap()[0].message, "old");
        manager
            .replace_user_source_announced(
                id,
                "local c=require('shoop_control'); c.register_global_event_cb(function() print('new') end)"
                    .to_owned(),
            )
            .unwrap();
        manager.dispatch_global_event();
        assert_eq!(manager.logs(id).unwrap()[0].message, "new");
        assert_eq!(manager.states()[0].id, id);
    }

    #[test]
    fn manager_rejects_bad_source_and_protects_non_user_records() {
        let mut manager = ScriptManager::new();
        assert!(manager
            .add_announced("bad", "this is not lua", ScriptKind::User, false)
            .is_err());
        assert!(manager.states().is_empty());
        let bundled = manager
            .add_announced("builtin", "return", ScriptKind::Bundled, false)
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
