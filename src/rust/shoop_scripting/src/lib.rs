use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::rc::Rc;

use anyhow::{anyhow, bail};
use mlua::{Function, Lua, Value};
use shoop_app_api::{ScriptId, ScriptKind, ScriptLifecycle, ScriptState};

mod control;
mod legacy_key_constants;

use control::{install_control_api, ScriptCallbacks};
pub use control::{
    ControlBridge, ControlLoop, ControlOperation, ControlSnapshot, ControlTrack,
    SharedControlBridge, CONTROL_FUNCTION_NAMES,
};
use legacy_key_constants::{LEGACY_KEY_CONSTANTS, LEGACY_MODIFIER_CONSTANTS};

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
}

impl ScriptManager {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            scripts: BTreeMap::new(),
            control: Rc::new(RefCell::new(ControlBridge::default())),
        }
    }

    pub fn set_control_snapshot(&mut self, snapshot: ControlSnapshot) {
        self.control.borrow_mut().snapshot = snapshot;
    }

    pub fn take_control_operations(&mut self) -> Vec<ControlOperation> {
        std::mem::take(&mut self.control.borrow_mut().operations)
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
            let record = self.scripts.get_mut(&id).unwrap();
            record.enabled = false;
            record.runtime = None;
            record.lifecycle = ScriptLifecycle::Inactive;
            record.latest_error = None;
            Ok(())
        }
    }

    pub fn start(&mut self, id: ScriptId) -> anyhow::Result<()> {
        let record = self.scripts.get_mut(&id).ok_or_else(|| stale_script(id))?;
        record.runtime = None;
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
        record.runtime = None;
        record.lifecycle = ScriptLifecycle::Inactive;
        record.latest_error = None;
        Ok(())
    }

    pub fn forget(&mut self, id: ScriptId) -> anyhow::Result<()> {
        let record = self.scripts.get(&id).ok_or_else(|| stale_script(id))?;
        if record.kind != ScriptKind::User {
            bail!("only user scripts can be forgotten")
        }
        self.scripts.remove(&id);
        Ok(())
    }

    pub fn states(&self) -> Vec<ScriptState> {
        self.scripts
            .values()
            .map(|record| ScriptState {
                id: record.id,
                name: record.name.clone(),
                kind: record.kind,
                enabled: record.enabled,
                lifecycle: record.lifecycle,
                documentation: record.documentation.clone(),
                latest_error: record.latest_error.clone(),
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

impl Default for ScriptManager {
    fn default() -> Self {
        Self::new()
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
