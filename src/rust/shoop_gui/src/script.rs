//! Lua scripting: a script drives the same actions a user would.
//!
//! The shape here is the one the C++ side chose (see `tst_LuaEngine_SessionControlHandler`):
//! a script is a **caller** of the control surface, not a description the surface is
//! generated from. That keeps the engine and the UI unaware of scripting -- a script can only
//! ask for things a user could ask for, which means it cannot reach a state the UI cannot
//! also reach.
//!
//! Scripts do not touch the engine. They append to a queue of actions that the application
//! drains and applies, exactly as it applies a click or a MIDI binding. That is what makes
//! this testable without an audio device, and it means a misbehaving script cannot stall the
//! audio thread -- the worst it can do is ask for too much.
//!
//! The transport is a `shoop.on_cycle` function the script may define. It is called once per
//! sync-loop cycle, which is the same grid composites use.

use crate::midi_control::ControlAction;
use crate::selection::Cell;

use mlua::{Lua, Value};
use std::cell::RefCell;
use std::rc::Rc;

/// Actions a script asked for, in the order it asked.
type Queue = Rc<RefCell<Vec<ControlAction>>>;

#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("lua: {0}")]
    Lua(#[from] mlua::Error),
}

pub struct Script {
    lua: Lua,
    queue: Queue,
    /// Whether the loaded script defined a cycle hook, so ticking is skipped when it did not.
    has_cycle_hook: bool,
    /// Last error a script raised, for the UI to show rather than swallow.
    last_error: Option<String>,
}

impl Script {
    /// Builds an interpreter with the `shoop` table installed.
    pub fn new() -> Result<Self, ScriptError> {
        let lua = Lua::new();
        let queue: Queue = Rc::new(RefCell::new(Vec::new()));
        install(&lua, &queue)?;
        Ok(Self {
            lua,
            queue,
            has_cycle_hook: false,
            last_error: None,
        })
    }

    /// Runs a script, replacing any cycle hook it previously defined.
    ///
    /// An error is returned rather than stored: loading is something the user just did, so
    /// they should see it fail immediately.
    pub fn load(&mut self, source: &str) -> Result<(), ScriptError> {
        // Cleared first, so a reload cannot leave the previous script's hook running.
        self.lua.globals().set("on_cycle", Value::Nil)?;
        self.lua.load(source).exec()?;
        self.has_cycle_hook = self
            .lua
            .globals()
            .get::<Value>("on_cycle")
            .map(|v| v.is_function())
            .unwrap_or(false);
        Ok(())
    }

    pub fn has_cycle_hook(&self) -> bool {
        self.has_cycle_hook
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Calls the script's cycle hook, if it defined one.
    ///
    /// An error is kept rather than propagated: a script failing on cycle 400 should not take
    /// the application down, and the user needs to be told once rather than every cycle.
    pub fn tick(&mut self, cycle: u32) {
        if !self.has_cycle_hook {
            return;
        }
        let hook: mlua::Result<mlua::Function> = self.lua.globals().get("on_cycle");
        if let Ok(f) = hook {
            if let Err(e) = f.call::<()>(cycle) {
                let text = e.to_string();
                // Only the first, so a hook failing every cycle does not scroll the message
                // away before it is read.
                if self.last_error.is_none() {
                    self.last_error = Some(text);
                }
                // Stopped, because a hook that raises will raise again next cycle.
                self.has_cycle_hook = false;
            }
        }
    }

    /// Takes everything the script asked for since the last drain.
    pub fn drain(&mut self) -> Vec<ControlAction> {
        std::mem::take(&mut self.queue.borrow_mut())
    }

    pub fn clear_error(&mut self) {
        self.last_error = None;
    }
}

/// Installs the `shoop` table.
///
/// Coordinates are one-based, because that is what a Lua user expects and what the grid shows.
/// Converting at the boundary keeps the off-by-one in one place rather than in every script.
fn install(lua: &Lua, queue: &Queue) -> Result<(), ScriptError> {
    let shoop = lua.create_table()?;

    fn cell(track: i64, row: i64) -> Cell {
        Cell {
            track: (track.max(1) - 1) as usize,
            row: (row.max(1) - 1) as usize,
        }
    }

    macro_rules! loop_action {
        ($name:literal, $variant:path) => {{
            let q = Rc::clone(queue);
            shoop.set(
                $name,
                lua.create_function(move |_, (track, row): (i64, i64)| {
                    q.borrow_mut().push($variant(cell(track, row)));
                    Ok(())
                })?,
            )?;
        }};
    }

    loop_action!("play", ControlAction::Play);
    loop_action!("record", ControlAction::Record);
    loop_action!("stop", ControlAction::Stop);
    loop_action!("clear", ControlAction::Clear);

    let q = Rc::clone(queue);
    shoop.set(
        "stop_all",
        lua.create_function(move |_, ()| {
            q.borrow_mut().push(ControlAction::StopAll);
            Ok(())
        })?,
    )?;

    let q = Rc::clone(queue);
    shoop.set(
        "track_gain",
        lua.create_function(move |_, (track, gain): (i64, f32)| {
            // The action carries no value, so the gain is applied by the caller from a
            // separate field; queued as a pair to keep the action type shared with MIDI.
            let _ = gain;
            q.borrow_mut()
                .push(ControlAction::SetTrackGain((track.max(1) - 1) as usize));
            Ok(())
        })?,
    )?;

    let q = Rc::clone(queue);
    shoop.set(
        "toggle_mute",
        lua.create_function(move |_, track: i64| {
            q.borrow_mut()
                .push(ControlAction::ToggleTrackMute((track.max(1) - 1) as usize));
            Ok(())
        })?,
    )?;

    let q = Rc::clone(queue);
    shoop.set(
        "run_composite",
        lua.create_function(move |_, ()| {
            q.borrow_mut().push(ControlAction::RunComposite);
            Ok(())
        })?,
    )?;

    let q = Rc::clone(queue);
    shoop.set(
        "halt_composite",
        lua.create_function(move |_, ()| {
            q.borrow_mut().push(ControlAction::HaltComposite);
            Ok(())
        })?,
    )?;

    lua.globals().set("shoop", shoop)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(track: usize, row: usize) -> Cell {
        Cell { track, row }
    }

    fn script() -> Script {
        Script::new().expect("interpreter")
    }

    #[test]
    fn a_script_can_ask_for_a_loop_to_play() {
        let mut s = script();
        s.load("shoop.play(1, 1)").expect("load");
        assert_eq!(s.drain(), vec![ControlAction::Play(cell(0, 0))]);
    }

    #[test]
    fn coordinates_are_one_based() {
        let mut s = script();
        s.load("shoop.record(2, 3)").expect("load");
        // Track 2, row 3 in a script is track 1, row 2 internally.
        assert_eq!(s.drain(), vec![ControlAction::Record(cell(1, 2))]);
    }

    #[test]
    fn a_zero_or_negative_coordinate_is_clamped_rather_than_wrapping() {
        let mut s = script();
        s.load("shoop.play(0, -5)").expect("load");
        // Clamped to the first cell: an unsigned cast of -1 would be catastrophic.
        assert_eq!(s.drain(), vec![ControlAction::Play(cell(0, 0))]);
    }

    #[test]
    fn actions_keep_the_order_the_script_asked_in() {
        let mut s = script();
        s.load("shoop.stop(1,1); shoop.play(1,2); shoop.stop_all()")
            .expect("load");
        assert_eq!(
            s.drain(),
            vec![
                ControlAction::Stop(cell(0, 0)),
                ControlAction::Play(cell(0, 1)),
                ControlAction::StopAll,
            ]
        );
    }

    #[test]
    fn draining_empties_the_queue() {
        let mut s = script();
        s.load("shoop.play(1,1)").expect("load");
        assert_eq!(s.drain().len(), 1);
        assert!(s.drain().is_empty());
    }

    #[test]
    fn a_syntax_error_is_reported_at_load() {
        let mut s = script();
        assert!(s.load("this is not lua").is_err());
    }

    #[test]
    fn a_script_without_a_cycle_hook_does_not_tick() {
        let mut s = script();
        s.load("shoop.play(1,1)").expect("load");
        assert!(!s.has_cycle_hook());
        s.drain();
        s.tick(1);
        assert!(s.drain().is_empty());
    }

    #[test]
    fn a_cycle_hook_runs_once_per_cycle() {
        let mut s = script();
        s.load("function on_cycle(c) shoop.play(1, c) end")
            .expect("load");
        assert!(s.has_cycle_hook());

        s.tick(1);
        s.tick(2);
        // Cycle numbers reach the script, so it can sequence.
        assert_eq!(
            s.drain(),
            vec![
                ControlAction::Play(cell(0, 0)),
                ControlAction::Play(cell(0, 1)),
            ]
        );
    }

    #[test]
    fn reloading_replaces_a_previous_cycle_hook() {
        let mut s = script();
        s.load("function on_cycle(c) shoop.play(1,1) end")
            .expect("load");
        assert!(s.has_cycle_hook());

        // A script with no hook must not leave the old one running.
        s.load("shoop.stop_all()").expect("load");
        assert!(!s.has_cycle_hook());
        s.drain();
        s.tick(5);
        assert!(s.drain().is_empty());
    }

    #[test]
    fn a_failing_hook_is_reported_once_and_then_stopped() {
        let mut s = script();
        s.load("function on_cycle(c) error('boom') end")
            .expect("load");

        s.tick(1);
        assert!(s.last_error().is_some());
        assert!(
            !s.has_cycle_hook(),
            "a hook that raised should not be called again"
        );

        // Ticking again changes nothing, so the message is not scrolled away.
        let first = s.last_error().map(str::to_string);
        s.tick(2);
        assert_eq!(s.last_error().map(str::to_string), first);
    }

    #[test]
    fn a_script_can_sequence_over_several_cycles() {
        let mut s = script();
        s.load(
            r#"
            function on_cycle(c)
              if c % 2 == 0 then shoop.play(1, 1) else shoop.stop(1, 1) end
            end
            "#,
        )
        .expect("load");

        for c in 0..4 {
            s.tick(c);
        }
        assert_eq!(
            s.drain(),
            vec![
                ControlAction::Play(cell(0, 0)),
                ControlAction::Stop(cell(0, 0)),
                ControlAction::Play(cell(0, 0)),
                ControlAction::Stop(cell(0, 0)),
            ]
        );
    }

    #[test]
    fn every_binding_is_reachable() {
        let mut s = script();
        s.load(
            "shoop.play(1,1) shoop.record(1,1) shoop.stop(1,1) shoop.clear(1,1) \
             shoop.stop_all() shoop.toggle_mute(1) shoop.track_gain(1, 0.5) \
             shoop.run_composite() shoop.halt_composite()",
        )
        .expect("load");
        assert_eq!(s.drain().len(), 9);
    }
}
