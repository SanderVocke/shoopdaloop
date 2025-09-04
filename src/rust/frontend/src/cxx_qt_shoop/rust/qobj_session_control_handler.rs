use crate::cxx_qt_shoop::qobj_session_control_handler_bridge::SessionControlHandlerLuaTarget;
use crate::cxx_qt_shoop::{
    qobj_lua_engine_bridge::ffi::LuaEngine, qobj_session_control_handler_bridge::ffi::*,
};
use crate::lua_callback::LuaCallback;
use crate::lua_engine::LuaEngineImpl;
use backend_bindings::LoopMode;
use common::logging::macros::*;
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QList, QString};
use cxx_qt_lib_shoop::connect::{connect, connect_or_report};
use cxx_qt_lib_shoop::connection_types;
use cxx_qt_lib_shoop::invokable::invoke;
use cxx_qt_lib_shoop::qobject::{
    qobject_property_bool, qobject_property_float, qobject_property_int, qobject_property_string,
    AsQObject,
};
use cxx_qt_lib_shoop::qpointer::{qpointer_from_qobject, QPointerQObject};
use cxx_qt_lib_shoop::qvariant_helpers::{
    qobject_ptr_to_qvariant, qvariant_to_qobject_ptr, qvariant_to_qvariantlist, qvariant_type_name,
    qvariantlist_to_qvariant,
};
use cxx_qt_lib_shoop::{qobject::FromQObject, qpointer::qpointer_to_qobject};
use itertools::Either;
use mlua::{FromLua, IntoLua};
use std::boxed::Box;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::pin::Pin;
use std::rc::{Rc, Weak};
shoop_log_unit!("Frontend.SessionControlHandler");

fn as_coords(val: &mlua::Value) -> Option<(i64, i64)> {
    if let mlua::Value::Table(table) = val {
        let len = table.len().unwrap_or(0);
        if len == 2
            && table
                .sequence_values::<mlua::Value>()
                .filter(|v| v.is_ok())
                .count()
                == 2
            && table
                .sequence_values::<mlua::Value>()
                .filter(|v| v.is_ok())
                .fold(true, |acc, v| {
                    acc && v.unwrap_or(mlua::Value::Nil).is_integer()
                })
        {
            let mut iter = table.sequence_values::<mlua::Value>();
            let first = iter.next().unwrap().unwrap_or(mlua::Value::Nil);
            let second = iter.next().unwrap().unwrap_or(mlua::Value::Nil);
            return Some((first.as_integer().unwrap(), second.as_integer().unwrap()));
        }
    }
    None
}

fn as_multi_coords(val: &mlua::Value) -> Option<Vec<(i64, i64)>> {
    if let mlua::Value::Table(table) = val {
        let len = table.len().unwrap_or(0);
        if len
            == table
                .sequence_values::<mlua::Value>()
                .filter(|v| v.is_ok())
                .count() as i64
        {
            let as_coords: Vec<Option<(i64, i64)>> = table
                .sequence_values::<mlua::Value>()
                .map(|v| match v {
                    Ok(v) => as_coords(&v),
                    Err(_) => None,
                })
                .collect();
            if as_coords.iter().fold(true, |acc, v| acc && v.is_some()) {
                return Some(as_coords.iter().map(|v| v.unwrap()).collect());
            }
        }
    }
    None
}

fn as_track_indices(val: &mlua::Value) -> Option<Vec<i64>> {
    if let mlua::Value::Table(table) = val {
        let len = table.len().unwrap_or(0);
        if len
            == table
                .sequence_values::<mlua::Value>()
                .filter(|v| v.is_ok())
                .count() as i64
        {
            let as_indices: Vec<Option<i64>> = table
                .sequence_values::<mlua::Value>()
                .map(|v| match v {
                    Ok(v) => match v {
                        mlua::Value::Integer(i) => Some(i as i64),
                        _ => None,
                    },
                    Err(_) => None,
                })
                .collect();
            if as_indices.iter().fold(true, |acc, v| acc && v.is_some()) {
                return Some(as_indices.iter().map(|v| v.unwrap()).collect());
            }
        }
    }
    None
}

fn loop_coords(l: *mut QObject) -> Result<(i64, i64), anyhow::Error> {
    if l.is_null() {
        return Err(anyhow::anyhow!("loop is null"));
    }
    unsafe {
        let x = qobject_property_int(&*l, "track_idx")?;
        let y = qobject_property_int(&*l, "idx_in_track")?;
        Ok((x as i64, y as i64))
    }
}

fn loop_coords_vec(l: *mut QObject) -> Result<Vec<i64>, anyhow::Error> {
    let tuple = loop_coords(l)?;
    Ok(vec![tuple.0, tuple.1])
}

fn loops_coords_vec(i: impl Iterator<Item = *mut QObject>) -> impl Iterator<Item = Vec<i64>> {
    i.filter_map(|l| loop_coords_vec(l).ok())
}

fn loop_int_prop(l: &*mut QObject, prop: &str) -> Result<i32, anyhow::Error> {
    if l.is_null() {
        return Err(anyhow::anyhow!("loop is null"));
    }
    unsafe {
        qobject_property_int(&**l, prop)
            .map_err(|e| anyhow::anyhow!("Could not get loop int prop: {e}"))
    }
}

fn loop_bool_prop(l: &*mut QObject, prop: &str) -> Result<bool, anyhow::Error> {
    if l.is_null() {
        return Err(anyhow::anyhow!("loop is null"));
    }
    unsafe {
        qobject_property_bool(&**l, prop)
            .map_err(|e| anyhow::anyhow!("Could not get loop bool prop: {e}"))
    }
}

fn loop_float_prop(l: &*mut QObject, prop: &str) -> Result<f32, anyhow::Error> {
    if l.is_null() {
        return Err(anyhow::anyhow!("loop is null"));
    }
    unsafe {
        qobject_property_float(&**l, prop)
            .map_err(|e| anyhow::anyhow!("Could not get loop float prop: {e}"))
    }
}

impl SessionControlHandlerLuaTarget {
    pub fn install_on_lua_engine(&mut self, engine: *mut QObject) {
        struct SessionControlCallback {
            weak_target: Weak<RefCell<SessionControlHandlerLuaTarget>>,
            weak_engine: Weak<RefCell<LuaEngineImpl>>,
            callable: Box<
                dyn Fn(
                    &SessionControlHandlerLuaTarget,
                    &mlua::Lua,
                    mlua::MultiValue,
                ) -> Result<mlua::Value, anyhow::Error>,
            >,
        }

        impl LuaCallback for SessionControlCallback {
            fn call(
                &self,
                lua: &mlua::Lua,
                args: mlua::MultiValue,
            ) -> Result<mlua::Value, anyhow::Error> {
                let strong_self = self
                    .weak_target
                    .upgrade()
                    .ok_or(anyhow::anyhow!("Session control target went out of scope"))?;
                let strong_self = strong_self.borrow();
                let strong_engine = self
                    .weak_engine
                    .upgrade()
                    .ok_or(anyhow::anyhow!("Lua engine went out of scope"))?;
                let strong_engine = strong_engine.borrow();
                (self.callable)(&strong_self, &strong_engine.lua, args)
            }
        }

        if let Err(e) = || -> Result<(), anyhow::Error> {
            let mut engine = unsafe { LuaEngine::from_qobject_mut_ptr(engine)? };
            let mut engine_rust = engine.as_mut().rust_mut();
            let wrapped_engine = engine_rust
                .engine
                .as_mut()
                .ok_or(anyhow::anyhow!("Wrapped engine not initialized"))?;

            let lua_module: mlua::Table = wrapped_engine
                .lua
                .borrow()
                .lua
                .create_table()
                .map_err(|e| anyhow::anyhow!("Could not create table: {e}"))?;

            let lua_constants: mlua::Table = wrapped_engine
                .lua
                .borrow()
                .lua
                .create_table()
                .map_err(|e| anyhow::anyhow!("Could not create table: {e}"))?;

            let map_const_err = |r: Result<(), mlua::Error>| {
                r.map_err(|e| anyhow::anyhow!("Could not set constant: {e}"))
            };

            // LoopMode
            map_const_err(lua_constants.set("LoopMode_Unknown", LoopMode::Unknown as i64))?;
            map_const_err(lua_constants.set("LoopMode_Stopped", LoopMode::Stopped as i64))?;
            map_const_err(lua_constants.set("LoopMode_Playing", LoopMode::Playing as i64))?;
            map_const_err(lua_constants.set("LoopMode_Recording", LoopMode::Recording as i64))?;
            map_const_err(lua_constants.set("LoopMode_Replacing", LoopMode::Replacing as i64))?;
            map_const_err(lua_constants.set(
                "LoopMode_PlayingDryThroughWet",
                LoopMode::PlayingDryThroughWet as i64,
            ))?;
            map_const_err(lua_constants.set(
                "LoopMode_RecordingDryIntoWet",
                LoopMode::RecordingDryIntoWet as i64,
            ))?;

            // Special arg values
            map_const_err(lua_constants.set("Loop_DontWaitForSync", -1))?;
            map_const_err(lua_constants.set("Loop_DontAlignToSyncImmediately", -1))?;

            map_const_err(lua_module.set("constants", lua_constants))?;

            // Macro for adding a callback method to the module repeatedly.
            macro_rules! add_callback {
                ($name: expr, $callback: expr) => {{
                    let weak_clone = self.weak_self.clone();
                    let callback = SessionControlCallback {
                        weak_engine: Rc::downgrade(&wrapped_engine.lua),
                        weak_target: weak_clone,
                        callable: Box::new($callback),
                    };
                    let callback_rc: Rc<Box<dyn LuaCallback>> = Rc::new(Box::new(callback));
                    let callback_lua = wrapped_engine.create_callback_fn($name, &callback_rc)?;
                    lua_module.set($name, callback_lua).map_err(|e| {
                        anyhow::anyhow!("Could not set module callback {}: {e}", $name)
                    })?;
                    self.callbacks.push(callback_rc);
                }};
            }

            // Even shorter version for directly forwarding to a member call
            // on self.
            macro_rules! add_self_callback {
                ($fn: ident) => {
                    add_callback!(stringify!($fn), |self_ref, lua, args| {
                        self_ref.$fn(lua, args)
                    })
                };
            }

            add_self_callback!(loop_count);
            add_self_callback!(loop_get_all);
            add_self_callback!(loop_get_which_selected);
            add_self_callback!(loop_get_which_targeted);
            add_self_callback!(loop_get_by_mode);
            add_self_callback!(loop_get_mode);
            add_self_callback!(loop_get_next_mode);
            add_self_callback!(loop_get_next_mode_delay);
            add_self_callback!(loop_get_length);
            add_self_callback!(loop_get_by_track);
            add_self_callback!(loop_transition);
            add_self_callback!(loop_trigger);
            add_self_callback!(loop_trigger_grab);
            add_self_callback!(loop_get_gain);
            add_self_callback!(loop_get_gain_fader);
            add_self_callback!(loop_get_balance);
            add_self_callback!(loop_record_n);
            add_self_callback!(loop_record_with_targeted);
            add_self_callback!(loop_set_gain);
            add_self_callback!(loop_set_gain_fader);
            add_self_callback!(loop_set_balance);
            add_self_callback!(loop_select);
            add_self_callback!(loop_target);
            add_self_callback!(loop_clear);
            add_self_callback!(loop_clear_all);
            add_self_callback!(loop_untarget_all);
            add_self_callback!(loop_toggle_targeted);
            add_self_callback!(loop_toggle_selected);
            add_self_callback!(loop_adopt_ringbuffers);
            add_self_callback!(loop_compose_add_to_end);

            add_self_callback!(track_get_gain);
            add_self_callback!(track_get_balance);
            add_self_callback!(track_get_gain_fader);
            add_self_callback!(track_get_input_gain);
            add_self_callback!(track_get_input_gain_fader);
            add_self_callback!(track_get_muted);
            add_self_callback!(track_set_muted);
            add_self_callback!(track_get_input_muted);
            add_self_callback!(track_set_input_muted);
            add_self_callback!(track_set_gain);
            add_self_callback!(track_set_balance);
            add_self_callback!(track_set_gain_fader);
            add_self_callback!(track_set_input_gain);
            add_self_callback!(track_set_input_gain_fader);

            add_self_callback!(set_apply_n_cycles);
            add_self_callback!(get_apply_n_cycles);
            add_self_callback!(set_solo);
            add_self_callback!(get_solo);
            add_self_callback!(set_sync_active);
            add_self_callback!(get_sync_active);
            add_self_callback!(set_play_after_record);
            add_self_callback!(get_play_after_record);
            add_self_callback!(set_default_recording_action);
            add_self_callback!(get_default_recording_action);

            wrapped_engine.set_toplevel("__shoop_control", lua_module)?;

            Ok(())
        }() {
            error!("Could not install on Lua engine: {e}");
        }
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.loop_count(loop_selector) -> int
    Count the amount of loops given by the selector.
    @shoop_lua_fn_docstring.end
    */
    fn loop_count(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("Expected 1 argument, got {}", args.len()));
        }
        Ok(mlua::Value::Integer(
            self.select_loops(lua, args.get(0).unwrap_or(&mlua::Value::Nil))?
                .filter(|p| !p.is_null())
                .count() as i64,
        ))
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.loop_get_all() -> [[x1,y1],[x2,y2],...]
    Get the coordinates of all loops.
    @shoop_lua_fn_docstring.end
    */
    fn loop_get_all(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 0 {
            return Err(anyhow::anyhow!("Expected 0 arguments, got {}", args.len()));
        }
        loops_coords_vec(self.all_loops_iter()?)
            .collect::<Vec<Vec<i64>>>()
            .into_lua(lua)
            .map_err(|e| anyhow::anyhow!("Could not convert into lua: {e}"))
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.loop_get_which_selected() -> [[x1,y1],[x2,y2],...]
    Get the coordinates of all currently selected loops.
    @shoop_lua_fn_docstring.end
    */
    fn loop_get_which_selected(
        &self,
        lua: &mlua::Lua,
        _args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        loops_coords_vec(self.selected_loops.iter().map(|p| p.clone()))
            .collect::<Vec<Vec<i64>>>()
            .into_lua(lua)
            .map_err(|e| anyhow::anyhow!("Could not convert loop coords to lua: {e}"))
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.loop_get_which_targeted() -> [x,y] | nil
    Get the coordinates of the currently targeted loop, or None if none are targeted.
    @shoop_lua_fn_docstring.end
    */
    fn loop_get_which_targeted(
        &self,
        lua: &mlua::Lua,
        _args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        match self.maybe_targeted_loop {
            Some(l) => loop_coords_vec(l)?
                .into_lua(lua)
                .map_err(|e| anyhow::anyhow!("Could not convert loop coords to lua: {e}")),
            None => Ok(mlua::Nil),
        }
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.loop_get_by_mode(mode) -> [[x1,y1],[x2,y2],...]
    Get the coordinates of all loops with the given mode.
    @shoop_lua_fn_docstring.end
    */
    fn loop_get_by_mode(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("Expected 1 argument, got {}", args.len()));
        }
        let mode: i64 = i64::from_lua(args.get(0).unwrap().clone(), lua)
            .map_err(|e| anyhow::anyhow!("arg is not an int: {e}"))?;
        loops_coords_vec(
            self.all_loops_iter()?
                .filter(|l| loop_int_prop(l, "mode").ok() == Some(mode as i32)),
        )
        .collect::<Vec<Vec<i64>>>()
        .into_lua(lua)
        .map_err(|e| anyhow::anyhow!("Could not convert into lua: {e}"))
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.loop_get_mode(loop_selector) -> list[LoopMode]
    Get the current mode of the specified loops.
    @shoop_lua_fn_docstring.end
    */
    fn loop_get_mode(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("Expected 1 argument, got {}", args.len()));
        }
        self.select_loops(lua, args.get(0).unwrap_or(&mlua::Value::Nil))?
            .map(|l| loop_int_prop(&l, "mode").unwrap_or(LoopMode::Unknown as i32))
            .collect::<Vec<i32>>()
            .into_lua(lua)
            .map_err(|e| anyhow::anyhow!("could not convert to lua: {e}"))
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.loop_get_next_mode(loop_selector) -> list[ LoopMode or nil ]
    For the specified loops, get the upcoming mode transition, if any.
    @shoop_lua_fn_docstring.end
    */
    fn loop_get_next_mode(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("Expected 1 argument, got {}", args.len()));
        }
        self.select_loops(lua, args.get(0).unwrap_or(&mlua::Value::Nil))?
            .map(|l| loop_int_prop(&l, "next_mode").unwrap_or(LoopMode::Unknown as i32))
            .collect::<Vec<i32>>()
            .into_lua(lua)
            .map_err(|e| anyhow::anyhow!("could not convert to lua: {e}"))
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.loop_get_next_mode_delay(loop_selector) -> list[ int or nil ]
    For the specified loops, get the upcoming mode transition delay in cycles, if any.
    @shoop_lua_fn_docstring.end
     */
    fn loop_get_next_mode_delay(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("Expected 1 argument, got {}", args.len()));
        }
        self.select_loops(lua, args.get(0).unwrap_or(&mlua::Value::Nil))?
            .map(|l| loop_int_prop(&l, "next_transition_delay").unwrap_or(-1))
            .collect::<Vec<i32>>()
            .into_lua(lua)
            .map_err(|e| anyhow::anyhow!("could not convert to lua: {e}"))
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.loop_get_length(loop_selector) -> list[int]
    Get the length of the specified loops in samples.
    @shoop_lua_fn_docstring.end
    */
    fn loop_get_length(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("Expected 1 argument, got {}", args.len()));
        }
        self.select_loops(lua, args.get(0).unwrap_or(&mlua::Value::Nil))?
            .map(|l| loop_int_prop(&l, "length").unwrap_or(0))
            .collect::<Vec<i32>>()
            .into_lua(lua)
            .map_err(|e| anyhow::anyhow!("could not convert to lua: {e}"))
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.loop_get_by_track(track) -> [[x1,y1],[x2,y2],...]
    Get the coordinates of all loops with the given mode.
    @shoop_lua_fn_docstring.end
    */
    fn loop_get_by_track(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("Expected 1 argument, got {}", args.len()));
        }
        let track_idx = match args.get(0).unwrap() {
            mlua::Value::Integer(track_idx) => *track_idx,
            _ => {
                return Err(anyhow::anyhow!("arg is not an integer"));
            }
        };
        loops_coords_vec(
            self.structured_loop_widget_references
                .keys()
                .filter(|(x, _y)| *x == track_idx)
                .map(|coords| self.structured_loop_widget_references.get(&coords).unwrap())
                .map(|qpointer| unsafe { qpointer_to_qobject(&qpointer) })
                .filter(|p| !p.is_null()),
        )
        .collect::<Vec<Vec<i64>>>()
        .into_lua(lua)
        .map_err(|e| anyhow::anyhow!("Could not convert coords: {e}"))
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.loop_transition(loop_selector, mode, maybe_cycles_delay, maybe_align_to_sync_at)
    Transition the given loops.
    Pass shoop_control.constants.Loop_DontWaitForSync and shoop_control.constants.Loop_DontAlignToSyncImmediately to maybe_cycles_delay and maybe_align_to_sync_at respectively, to disable them.
    @shoop_lua_fn_docstring.end
    */
    fn loop_transition(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 4 {
            return Err(anyhow::anyhow!("Expected 4 arguments, got {}", args.len()));
        }
        let selector = args.get(0).unwrap_or(&mlua::Value::Nil);
        let mode = match args.get(1).unwrap_or(&mlua::Value::Nil) {
            mlua::Value::Integer(mode) => *mode as i32,
            _ => {
                return Err(anyhow::anyhow!("arg 2 is not a mode"));
            }
        };
        let maybe_cycles_delay = match args.get(2).unwrap_or(&mlua::Value::Nil) {
            mlua::Value::Integer(maybe_cycles_delay) => *maybe_cycles_delay as i32,
            _ => {
                return Err(anyhow::anyhow!("arg 3 is not a cycles delay"));
            }
        };
        let maybe_align_to_sync_at = match args.get(3).unwrap_or(&mlua::Value::Nil) {
            mlua::Value::Integer(maybe_align_to_sync_at) => *maybe_align_to_sync_at as i32,
            _ => {
                return Err(anyhow::anyhow!("arg 3 is not a sync at point"));
            }
        };
        self.select_loops(lua, selector)?.try_for_each(|l| unsafe {
            if l.is_null() {
                warn!("loop_transition: loop is null");
                return Ok(());
            }
            invoke::<_, (), _>(
                &mut *l,
                "transition(QVariant,QVariant,QVariant,QVariant)",
                connection_types::QUEUED_CONNECTION,
                &(
                    QVariant::from(&mode),
                    QVariant::from(&maybe_cycles_delay),
                    QVariant::from(&maybe_align_to_sync_at),
                    QVariant::from(&true),
                ),
            )
        })?;
        Ok(mlua::Value::Nil)
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.loop_trigger(loop_selector, mode)
    Trigger the loop with the given mode. Equivalent to pressing the loop's button in the UI.
    That means the way the trigger is interpreted also depends on the global controls for e.g. sync.
    @shoop_lua_fn_docstring.end
    */
    fn loop_trigger(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 2 {
            return Err(anyhow::anyhow!("Expected 2 arguments, got {}", args.len()));
        }
        let selector = args.get(0).unwrap_or(&mlua::Value::Nil);
        let mode = match args.get(1).unwrap_or(&mlua::Value::Nil) {
            mlua::Value::Integer(mode) => *mode as i32,
            _ => {
                return Err(anyhow::anyhow!("arg 2 is not a mode"));
            }
        };
        self.select_loops(lua, selector)?.try_for_each(|l| unsafe {
            if l.is_null() {
                warn!("loop_trigger: loop is null");
                return Ok(());
            }
            invoke::<_, (), _>(
                &mut *l,
                "trigger_mode_button(QVariant)",
                connection_types::QUEUED_CONNECTION,
                &(QVariant::from(&mode)),
            )
        })?;
        Ok(mlua::Value::Nil)
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.loop_trigger_grab(loop_selector, mode)
    Trigger a ringbuffer grab on the given loop. Equivalent to pressing the grab button.
    @shoop_lua_fn_docstring.end
    */
    fn loop_trigger_grab(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("Expected 1 argument, got {}", args.len()));
        }
        let selector = args.get(0).unwrap_or(&mlua::Value::Nil);
        self.select_loops(lua, selector)?.try_for_each(|l| unsafe {
            if l.is_null() {
                warn!("loop_trigger_grab: loop is null");
                return Ok(());
            }
            invoke::<_, (), _>(
                &mut *l,
                "on_grab_clicked()",
                connection_types::QUEUED_CONNECTION,
                &(),
            )
        })?;
        Ok(mlua::Value::Nil)
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.loop_get_gain(loop_selector) -> list[float]
    Get the output audio gain of the specified loops as a gain factor.
    @shoop_lua_fn_docstring.end
    */
    fn loop_get_gain(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("Expected 1 argument, got {}", args.len()));
        }
        self.select_loops(lua, args.get(0).unwrap_or(&mlua::Value::Nil))?
            .map(|l| loop_float_prop(&l, "last_pushed_gain").unwrap_or(0.0))
            .collect::<Vec<f32>>()
            .into_lua(lua)
            .map_err(|e| anyhow::anyhow!("could not convert to lua: {e}"))
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.loop_get_gain(loop_selector) -> list[float]
    Get the output audio gain fader position as a fraction of its total range (0-1) of the given loop.
    @shoop_lua_fn_docstring.end
    */
    fn loop_get_gain_fader(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("Expected 1 argument, got {}", args.len()));
        }
        self.select_loops(lua, args.get(0).unwrap_or(&mlua::Value::Nil))?
            .map(|l| loop_float_prop(&l, "gain_fader").unwrap_or(0.0))
            .collect::<Vec<f32>>()
            .into_lua(lua)
            .map_err(|e| anyhow::anyhow!("could not convert to lua: {e}"))
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.loop_get_balance(loop_selector) -> list[float]
    Get the output audio balance of the specified stereo loops.
    @shoop_lua_fn_docstring.end
    */
    fn loop_get_balance(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("Expected 1 argument, got {}", args.len()));
        }
        self.select_loops(lua, args.get(0).unwrap_or(&mlua::Value::Nil))?
            .map(|l| loop_float_prop(&l, "last_pushed_stereo_balance").unwrap_or(0.0))
            .collect::<Vec<f32>>()
            .into_lua(lua)
            .map_err(|e| anyhow::anyhow!("could not convert to lua: {e}"))
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.loop_record_n(loop_selector, n_cycles, cycles_delay)
    Record the given loops for N cycles synchronously.
    @shoop_lua_fn_docstring.end
    */
    fn loop_record_n(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 3 {
            return Err(anyhow::anyhow!("Expected 3 arguments, got {}", args.len()));
        }
        let selector = args.get(0).unwrap_or(&mlua::Value::Nil);
        let n = match args.get(1).unwrap_or(&mlua::Value::Nil) {
            mlua::Value::Integer(n) => *n as i32,
            _ => {
                return Err(anyhow::anyhow!("arg 2 is not an integer"));
            }
        };
        let cycles_delay = match args.get(2).unwrap_or(&mlua::Value::Nil) {
            mlua::Value::Integer(cycles_delay) => *cycles_delay as i32,
            _ => {
                return Err(anyhow::anyhow!("arg 3 is not an integer"));
            }
        };
        self.select_loops(lua, selector)?.try_for_each(|l| unsafe {
            if l.is_null() {
                warn!("loop_record_n: loop is null");
                return Ok(());
            }
            invoke::<_, (), _>(
                &mut *l,
                "record_n(QVariant,QVariant)",
                connection_types::QUEUED_CONNECTION,
                &(QVariant::from(&cycles_delay), QVariant::from(&n)),
            )
        })?;
        Ok(mlua::Value::Nil)
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.loop_record_with_targeted(loop_selector)
    Record the given loops in sync with the currently targeted loop.
    @shoop_lua_fn_docstring.end
    */
    fn loop_record_with_targeted(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("Expected 1 argument, got {}", args.len()));
        }
        let selector = args.get(0).unwrap_or(&mlua::Value::Nil);
        self.select_loops(lua, selector)?.try_for_each(|l| unsafe {
            if l.is_null() {
                warn!("loop_record_with_targeted: loop is null");
                return Ok(());
            }
            invoke::<_, (), _>(
                &mut *l,
                "record_with_targeted()",
                connection_types::QUEUED_CONNECTION,
                &(),
            )
        })?;
        Ok(mlua::Value::Nil)
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.loop_set_gain(loop_selector, gain)
    Set the output audio gain of the specified loops as a gain factor.
    @shoop_lua_fn_docstring.end
    */
    fn loop_set_gain(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 2 {
            return Err(anyhow::anyhow!("Expected 2 arguments, got {}", args.len()));
        }
        let selector = args.get(0).unwrap_or(&mlua::Value::Nil);
        let gain = match args.get(1).unwrap_or(&mlua::Value::Nil) {
            mlua::Value::Number(gain) => *gain as f32,
            _ => {
                return Err(anyhow::anyhow!("arg 2 is not a gain number"));
            }
        };
        self.select_loops(lua, selector)?.try_for_each(|l| unsafe {
            if l.is_null() {
                warn!("loop_set_gain: loop is null");
                return Ok(());
            }
            invoke::<_, (), _>(
                &mut *l,
                "push_gain(QVariant)",
                connection_types::QUEUED_CONNECTION,
                &(QVariant::from(&gain)),
            )
        })?;
        Ok(mlua::Value::Nil)
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.loop_set_gain(loop_selector)
    Set the output audio gain fader position as a fraction of its total range (0-1) of the given loop.
    @shoop_lua_fn_docstring.end
    */
    fn loop_set_gain_fader(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 2 {
            return Err(anyhow::anyhow!("Expected 2 arguments, got {}", args.len()));
        }
        let selector = args.get(0).unwrap_or(&mlua::Value::Nil);
        let gain = match args.get(1).unwrap_or(&mlua::Value::Nil) {
            mlua::Value::Number(gain) => *gain as f32,
            _ => {
                return Err(anyhow::anyhow!("arg 2 is not a gain number"));
            }
        };
        self.select_loops(lua, selector)?.try_for_each(|l| unsafe {
            if l.is_null() {
                warn!("loop_set_gain_fader: loop is null");
                return Ok(());
            }
            invoke::<_, (), _>(
                &mut *l,
                "set_gain_fader(QVariant)",
                connection_types::QUEUED_CONNECTION,
                &(QVariant::from(&gain)),
            )
        })?;
        Ok(mlua::Value::Nil)
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.loop_set_balance(loop_selector, balance)
    Set the audio output balance for the specified loops.
    @shoop_lua_fn_docstring.end
    */
    fn loop_set_balance(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 2 {
            return Err(anyhow::anyhow!("Expected 2 arguments, got {}", args.len()));
        }
        let selector = args.get(0).unwrap_or(&mlua::Value::Nil);
        let balance = match args.get(1).unwrap_or(&mlua::Value::Nil) {
            mlua::Value::Number(gain) => *gain as f32,
            _ => {
                return Err(anyhow::anyhow!("arg 2 is not a gain number"));
            }
        };
        self.select_loops(lua, selector)?.try_for_each(|l| unsafe {
            if l.is_null() {
                warn!("push_stereo_balance: loop is null");
                return Ok(());
            }
            invoke::<_, (), _>(
                &mut *l,
                "push_stereo_balance(QVariant)",
                connection_types::QUEUED_CONNECTION,
                &(QVariant::from(&balance)),
            )
        })?;
        Ok(mlua::Value::Nil)
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.loop_select(loop_selector, deselect_others)
    Select the specified loops. If deselect_others is true, all other loops are deselected.
    @shoop_lua_fn_docstring.end
    */
    fn loop_select(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 2 {
            return Err(anyhow::anyhow!("Expected 2 arguments, got {}", args.len()));
        }
        let selector = args.get(0).unwrap_or(&mlua::Value::Nil);
        let clear = match args.get(1).unwrap_or(&mlua::Value::Nil) {
            mlua::Value::Boolean(clear) => *clear,
            _ => {
                return Err(anyhow::anyhow!("arg 2 is not a boolean"));
            }
        };
        let mut loops: QList_QVariant = QList::default();
        self.select_loops(lua, selector)?.for_each(|l| {
            loops.append(qobject_ptr_to_qvariant(&l).unwrap_or(QVariant::default()));
        });

        unsafe {
            if self.session.is_null() {
                warn!("loop_select: session is null");
                return Ok(mlua::Value::Nil);
            }
            invoke::<_, (), _>(
                &mut *self.session,
                "select_loops(QVariant,QVariant)",
                connection_types::QUEUED_CONNECTION,
                &(qvariantlist_to_qvariant(&loops)?, QVariant::from(&clear)),
            )?;
        }

        Ok(mlua::Value::Nil)
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.loop_target(loop_selector)
    Target the specified loop. If the selector specifies more than one loop, a single loop in the set is chosen arbitrarily.
    If nil or no loop is passed, the targeted loop is cleared.
    @shoop_lua_fn_docstring.end
    */
    fn loop_target(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("Expected 1 argument, got {}", args.len()));
        }
        let selector = args.get(0).unwrap_or(&mlua::Value::Nil);
        let l = self
            .select_loops(lua, selector)?
            .nth(0)
            .unwrap_or(std::ptr::null_mut());

        unsafe {
            if self.session.is_null() {
                warn!("loop_target: session is null");
                return Ok(mlua::Value::Nil);
            }
            invoke::<_, (), _>(
                &mut *self.session,
                "target_loop(QVariant)",
                connection_types::QUEUED_CONNECTION,
                &(qobject_ptr_to_qvariant(&l).unwrap_or(QVariant::default())),
            )?;
        }
        Ok(mlua::Value::Nil)
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.loop_clear(loop_selector)
    Clear the given loops.
    @shoop_lua_fn_docstring.end
    */
    fn loop_clear(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("Expected 1 argument, got {}", args.len()));
        }
        let selector = args.get(0).unwrap_or(&mlua::Value::Nil);
        self.select_loops(lua, selector)?.try_for_each(|l| unsafe {
            if l.is_null() {
                warn!("loop_clear: loop is null");
                return Ok(());
            }
            invoke::<_, (), _>(
                &mut *l,
                "clear(QVariant,QVariant)",
                connection_types::QUEUED_CONNECTION,
                &(QVariant::from(&0), QVariant::from(&false)),
            )
        })?;
        Ok(mlua::Value::Nil)
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.loop_clear_all()
    Clear all loops.
    @shoop_lua_fn_docstring.end
    */
    fn loop_clear_all(
        &self,
        _lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 0 {
            return Err(anyhow::anyhow!("Expected 0 arguments, got {}", args.len()));
        }
        self.all_loops_iter()?.try_for_each(|l| unsafe {
            if l.is_null() {
                warn!("loop_clear_all: loop is null");
                return Ok(());
            }
            invoke::<_, (), _>(
                &mut *l,
                "clear(QVariant,QVariant)",
                connection_types::QUEUED_CONNECTION,
                &(QVariant::from(&0), QVariant::from(&false)),
            )
        })?;
        Ok(mlua::Value::Nil)
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.loop_clear_all()
    Untarget all loops.
    @shoop_lua_fn_docstring.end
    */
    fn loop_untarget_all(
        &self,
        _lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 0 {
            return Err(anyhow::anyhow!("Expected 0 argument, got {}", args.len()));
        }
        let l = self
            .all_loops_iter()?
            .nth(0)
            .ok_or(anyhow::anyhow!("No loop found to help untarget"))?;
        unsafe {
            if l.is_null() {
                warn!("loop_untarget_all: loop is null");
                return Ok(mlua::Value::Nil);
            }
            invoke::<_, (), _>(
                &mut *l,
                "untarget()",
                connection_types::QUEUED_CONNECTION,
                &(),
            )?;
        }
        Ok(mlua::Value::Nil)
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.loop_toggle_selected(loop_selector)
    Toggle selection on the specified loops.
    @shoop_lua_fn_docstring.end
    */
    fn loop_toggle_selected(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("Expected 1 argument, got {}", args.len()));
        }
        let selector = args.get(0).unwrap_or(&mlua::Value::Nil);
        let l = self
            .select_loops(lua, selector)?
            .nth(0)
            .ok_or(anyhow::anyhow!("No loop found to toggle selected"))?;
        unsafe {
            if l.is_null() {
                warn!("loop_toggle_selected: loop is null");
                return Ok(mlua::Value::Nil);
            }
            invoke::<_, (), _>(
                &mut *l,
                "toggle_selected(QVariant)",
                connection_types::QUEUED_CONNECTION,
                &(QVariant::from(&false)),
            )?;
        }
        Ok(mlua::Value::Nil)
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.loop_toggle_targeted(loop_selector)
    Target the specified loop or untarget it if already targeted. If the selector specifies more than one loop, a single loop in the set is chosen arbitrarily.
    If nil or no loop is passed, the targeted loop is cleared.
    @shoop_lua_fn_docstring.end
    */
    fn loop_toggle_targeted(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("Expected 1 argument, got {}", args.len()));
        }
        let selector = args.get(0).unwrap_or(&mlua::Value::Nil);
        let l = self
            .select_loops(lua, selector)?
            .nth(0)
            .ok_or(anyhow::anyhow!("No loop found to toggle targeted"))?;
        unsafe {
            if l.is_null() {
                warn!("loop_toggle_targeted: loop is null");
                return Ok(mlua::Value::Nil);
            }
            invoke::<_, (), _>(
                &mut *l,
                "toggle_targeted()",
                connection_types::QUEUED_CONNECTION,
                &(),
            )?;
        }
        Ok(mlua::Value::Nil)
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.loop_adopt_ringbuffers(loop_selector, reverse_cycle_start, cycles_length, go_to_cycle, go_to_mode)
    For all channels in the given loops, grab the data currently in the ringbuffer and
    set it as the content (i.e. after-the-fact-recording or "grab").
    reverse_cycle_start sets the start offset for playback. 0 means to play what was being
    recorded in the current sync loop cycle, 1 means start from the previous cycle, etc.
    go_to_cycle and go_to_mode can control the cycle and mode the loop will have right after adopting.
    cycles_length sets the loop length.
    @shoop_lua_fn_docstring.end
    */
    fn loop_adopt_ringbuffers(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 5 {
            return Err(anyhow::anyhow!("Expected 5 arguments, got {}", args.len()));
        }
        let selector = args.get(0).unwrap_or(&mlua::Value::Nil);
        let reverse_start_cycle = match args.get(1).unwrap_or(&mlua::Value::Nil) {
            mlua::Value::Integer(reverse_start_cycle) => *reverse_start_cycle as i32,
            _ => {
                return Err(anyhow::anyhow!("arg 2 is not an integer"));
            }
        };
        let cycles_length = match args.get(2).unwrap_or(&mlua::Value::Nil) {
            mlua::Value::Integer(cycles_length) => *cycles_length as i32,
            _ => {
                return Err(anyhow::anyhow!("arg 3 is not an integer"));
            }
        };
        let go_to_cycle = match args.get(3).unwrap_or(&mlua::Value::Nil) {
            mlua::Value::Integer(go_to_cycle) => *go_to_cycle as i32,
            _ => {
                return Err(anyhow::anyhow!("arg 4 is not an integer"));
            }
        };
        let go_to_mode = match args.get(4).unwrap_or(&mlua::Value::Nil) {
            mlua::Value::Integer(go_to_mode) => *go_to_mode as i32,
            _ => {
                return Err(anyhow::anyhow!("arg 5 is not an integer"));
            }
        };
        self.select_loops(lua, selector)?.try_for_each(|l| unsafe {
            if l.is_null() {
                warn!("loop_transition: loop is null");
                return Ok(());
            }
            invoke::<_, (), _>(
                &mut *l,
                "adopt_ringbuffers(QVariant,QVariant,QVariant,QVariant)",
                connection_types::QUEUED_CONNECTION,
                &(
                    QVariant::from(&reverse_start_cycle),
                    QVariant::from(&cycles_length),
                    QVariant::from(&go_to_cycle),
                    QVariant::from(&go_to_mode),
                ),
            )
        })?;
        Ok(mlua::Value::Nil)
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.loop_compose_add_to_end(loop_selector, loop_selector, parallel)
    Add a loop to the a composition. The first argument is the singular target loop in which
    the composition is stored. If empty, a composition is created.
    The second argument is the loop that should be added.
    The third argument is whether the addition should be parallel to the existing composition.
    If false, the loop is added to the end.
    @shoop_lua_fn_docstring.end
    */
    fn loop_compose_add_to_end(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 3 {
            return Err(anyhow::anyhow!("Expected 3 arguments, got {}", args.len()));
        }
        let target_selector = args.get(0).unwrap_or(&mlua::Value::Nil);
        let add_selector = args.get(1).unwrap_or(&mlua::Value::Nil);
        let parallel = match args.get(2).unwrap_or(&mlua::Value::Nil) {
            mlua::Value::Boolean(parallel) => *parallel,
            _ => {
                return Err(anyhow::anyhow!("arg 3 is not a boolean"));
            }
        };
        let target_loop = self
            .select_loops(lua, target_selector)?
            .next()
            .ok_or(anyhow::anyhow!("No target loop found"))?;
        let add_loop = self
            .select_loops(lua, add_selector)?
            .next()
            .ok_or(anyhow::anyhow!("No loop to add found"))?;

        unsafe {
            if target_loop.is_null() {
                warn!("loop_compose_add_to_end: target loop is null");
                return Ok(mlua::Value::Nil);
            }
            invoke::<_, (), _>(
                &mut *target_loop,
                "compose_add_to_end(QVariant,QVariant)",
                connection_types::QUEUED_CONNECTION,
                &(
                    qobject_ptr_to_qvariant(&add_loop)?,
                    QVariant::from(&parallel),
                ),
            )?;
        }
        Ok(mlua::Value::Nil)
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.track_get_gain(track_selector) -> list[float]
    Get the gain of the given track(s) as a gain factor.
    @shoop_lua_fn_docstring.end
    */
    fn track_get_gain(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("Expected 1 argument, got {}", args.len()));
        }
        self.select_tracks(lua, args.get(0).unwrap_or(&mlua::Value::Nil))?
            .map(|l| loop_float_prop(&l, "last_pushed_gain").unwrap_or(0.0))
            .collect::<Vec<f32>>()
            .into_lua(lua)
            .map_err(|e| anyhow::anyhow!("could not convert to lua: {e}"))
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.track_get_balance(track_selector) -> list[float]
    Get the balance of the given track(s) as a value between -1 and 1.
    @shoop_lua_fn_docstring.end
    */
    fn track_get_balance(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("Expected 1 argument, got {}", args.len()));
        }
        self.select_tracks(lua, args.get(0).unwrap_or(&mlua::Value::Nil))?
            .map(|l| loop_float_prop(&l, "last_pushed_out_stereo_balance").unwrap_or(0.0))
            .collect::<Vec<f32>>()
            .into_lua(lua)
            .map_err(|e| anyhow::anyhow!("could not convert to lua: {e}"))
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.track_get_gain_fader(track_selector) -> list[float]
    Get the gain of the given track(s) as a fraction of its total range (0-1).
    @shoop_lua_fn_docstring.end
    */
    fn track_get_gain_fader(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("Expected 1 argument, got {}", args.len()));
        }
        self.select_tracks(lua, args.get(0).unwrap_or(&mlua::Value::Nil))?
            .map(|l| loop_float_prop(&l, "gain_fader_position").unwrap_or(0.0))
            .collect::<Vec<f32>>()
            .into_lua(lua)
            .map_err(|e| anyhow::anyhow!("could not convert to lua: {e}"))
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.track_get_input_gain(track_selector) -> list[float]
    Get the input gain of the given track(s) as a gain factor.
    @shoop_lua_fn_docstring.end
    */
    fn track_get_input_gain(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("Expected 1 argument, got {}", args.len()));
        }
        self.select_tracks(lua, args.get(0).unwrap_or(&mlua::Value::Nil))?
            .map(|l| loop_float_prop(&l, "last_pushed_in_gain").unwrap_or(0.0))
            .collect::<Vec<f32>>()
            .into_lua(lua)
            .map_err(|e| anyhow::anyhow!("could not convert to lua: {e}"))
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.track_get_input_gain_fader(track_selector) -> list[float]
    Get the input gain of the given track(s) as a fraction of its total range (0-1).
    @shoop_lua_fn_docstring.end
    */
    fn track_get_input_gain_fader(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("Expected 1 argument, got {}", args.len()));
        }
        self.select_tracks(lua, args.get(0).unwrap_or(&mlua::Value::Nil))?
            .map(|l| loop_float_prop(&l, "input_fader_position").unwrap_or(0.0))
            .collect::<Vec<f32>>()
            .into_lua(lua)
            .map_err(|e| anyhow::anyhow!("could not convert to lua: {e}"))
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.track_get_muted(track_selector) -> list[bool]
    Get whether the given track(s) is/are muted.
    @shoop_lua_fn_docstring.end
    */
    fn track_get_muted(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("Expected 1 argument, got {}", args.len()));
        }
        self.select_tracks(lua, args.get(0).unwrap_or(&mlua::Value::Nil))?
            .map(|l| loop_bool_prop(&l, "mute").unwrap_or(false))
            .collect::<Vec<bool>>()
            .into_lua(lua)
            .map_err(|e| anyhow::anyhow!("could not convert to lua: {e}"))
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.track_set_muted(track_selector, bool)
    Set whether the given track is muted.
    @shoop_lua_fn_docstring.end
    */
    fn track_set_muted(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 2 {
            return Err(anyhow::anyhow!("Expected 2 arguments, got {}", args.len()));
        }
        let selector = args.get(0).unwrap_or(&mlua::Value::Nil);
        let muted = match args.get(1).unwrap_or(&mlua::Value::Nil) {
            mlua::Value::Boolean(muted) => *muted,
            _ => {
                return Err(anyhow::anyhow!("arg 2 is not a boolean"));
            }
        };
        self.select_tracks(lua, selector)?
            .try_for_each(|t| unsafe {
                if t.is_null() {
                    warn!("track_set_muted: track is null");
                    return Ok(());
                }
                invoke::<_, (), _>(
                    &mut *t,
                    "set_mute(QVariant)",
                    connection_types::QUEUED_CONNECTION,
                    &(QVariant::from(&muted)),
                )
            })?;
        Ok(mlua::Value::Nil)
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.track_get_input_muted(track_selector) -> list[bool]
    Get whether the given tracks' input(s) is/are muted.
    @shoop_lua_fn_docstring.end
    */
    fn track_get_input_muted(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("Expected 1 argument, got {}", args.len()));
        }
        self.select_tracks(lua, args.get(0).unwrap_or(&mlua::Value::Nil))?
            .map(|l| !loop_bool_prop(&l, "monitor").unwrap_or(true))
            .collect::<Vec<bool>>()
            .into_lua(lua)
            .map_err(|e| anyhow::anyhow!("could not convert to lua: {e}"))
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.track_set_input_muted(track_selector, muted)
    Set whether the given track's input is muted.
    @shoop_lua_fn_docstring.end
    */
    fn track_set_input_muted(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 2 {
            return Err(anyhow::anyhow!("Expected 2 arguments, got {}", args.len()));
        }
        let selector = args.get(0).unwrap_or(&mlua::Value::Nil);
        let muted = match args.get(1).unwrap_or(&mlua::Value::Nil) {
            mlua::Value::Boolean(muted) => *muted,
            _ => {
                return Err(anyhow::anyhow!("arg 2 is not a boolean"));
            }
        };
        let monitor = !muted;
        self.select_tracks(lua, selector)?
            .try_for_each(|t| unsafe {
                if t.is_null() {
                    warn!("track_set_input_muted: track is null");
                    return Ok(());
                }
                invoke::<_, (), _>(
                    &mut *t,
                    "set_monitor(QVariant)",
                    connection_types::QUEUED_CONNECTION,
                    &(QVariant::from(&monitor)),
                )
            })?;
        Ok(mlua::Value::Nil)
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.track_set_gain(track_selector, vol)
    Set the given track's gain as a gain factor.
    @shoop_lua_fn_docstring.end
    */
    fn track_set_gain(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 2 {
            return Err(anyhow::anyhow!("Expected 2 arguments, got {}", args.len()));
        }
        let selector = args.get(0).unwrap_or(&mlua::Value::Nil);
        let gain = match args.get(1).unwrap_or(&mlua::Value::Nil) {
            mlua::Value::Number(gain) => *gain as f32,
            _ => {
                return Err(anyhow::anyhow!("arg 2 is not a float"));
            }
        };
        self.select_tracks(lua, selector)?
            .try_for_each(|t| unsafe {
                if t.is_null() {
                    warn!("set_gain: track is null");
                    return Ok(());
                }
                invoke::<_, (), _>(
                    &mut *t,
                    "set_gain(QVariant)",
                    connection_types::QUEUED_CONNECTION,
                    &(QVariant::from(&gain)),
                )
            })?;
        Ok(mlua::Value::Nil)
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.track_set_balance(track_selector, val)
    Set the given track's balance as a value between -1 and 1.
    @shoop_lua_fn_docstring.end
    */
    fn track_set_balance(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 2 {
            return Err(anyhow::anyhow!("Expected 2 arguments, got {}", args.len()));
        }
        let selector = args.get(0).unwrap_or(&mlua::Value::Nil);
        let balance = match args.get(1).unwrap_or(&mlua::Value::Nil) {
            mlua::Value::Number(balance) => *balance as f32,
            _ => {
                return Err(anyhow::anyhow!("arg 2 is not a float"));
            }
        };
        self.select_tracks(lua, selector)?
            .try_for_each(|t| unsafe {
                if t.is_null() {
                    warn!("track_set_balance: track is null");
                    return Ok(());
                }
                invoke::<_, (), _>(
                    &mut *t,
                    "set_balance(QVariant)",
                    connection_types::QUEUED_CONNECTION,
                    &(QVariant::from(&balance)),
                )
            })?;
        Ok(mlua::Value::Nil)
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.track_set_gain_fader(track_selector, vol)
    Set the given track's gain as a fraction of its total range (0-1).
    @shoop_lua_fn_docstring.end
    */
    fn track_set_gain_fader(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 2 {
            return Err(anyhow::anyhow!("Expected 2 arguments, got {}", args.len()));
        }
        let selector = args.get(0).unwrap_or(&mlua::Value::Nil);
        let gain = match args.get(1).unwrap_or(&mlua::Value::Nil) {
            mlua::Value::Number(gain) => *gain as f32,
            _ => {
                return Err(anyhow::anyhow!("arg 2 is not a float"));
            }
        };
        self.select_tracks(lua, selector)?
            .try_for_each(|t| unsafe {
                if t.is_null() {
                    warn!("track_set_gain_fader: track is null");
                    return Ok(());
                }
                invoke::<_, (), _>(
                    &mut *t,
                    "set_gain_fader(QVariant)",
                    connection_types::QUEUED_CONNECTION,
                    &(QVariant::from(&gain)),
                )
            })?;
        Ok(mlua::Value::Nil)
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.track_set_input_gain(track_selector, vol)
    Set the given track's input gain as a gain factor.
    @shoop_lua_fn_docstring.end
    */
    fn track_set_input_gain(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 2 {
            return Err(anyhow::anyhow!("Expected 2 arguments, got {}", args.len()));
        }
        let selector = args.get(0).unwrap_or(&mlua::Value::Nil);
        let gain = match args.get(1).unwrap_or(&mlua::Value::Nil) {
            mlua::Value::Number(gain) => *gain as f32,
            _ => {
                return Err(anyhow::anyhow!("arg 2 is not a float"));
            }
        };
        self.select_tracks(lua, selector)?
            .try_for_each(|t| unsafe {
                if t.is_null() {
                    warn!("track_set_input_gain: track is null");
                    return Ok(());
                }
                invoke::<_, (), _>(
                    &mut *t,
                    "set_input_gain(QVariant)",
                    connection_types::QUEUED_CONNECTION,
                    &(QVariant::from(&gain)),
                )
            })?;
        Ok(mlua::Value::Nil)
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.track_set_input_gain_fader(track_selector, vol)
    Set the given track's input gain as a fraction of its total range (0-1).
    @shoop_lua_fn_docstring.end
    */
    fn track_set_input_gain_fader(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 2 {
            return Err(anyhow::anyhow!("Expected 2 arguments, got {}", args.len()));
        }
        let selector = args.get(0).unwrap_or(&mlua::Value::Nil);
        let gain = match args.get(1).unwrap_or(&mlua::Value::Nil) {
            mlua::Value::Number(gain) => *gain as f32,
            _ => {
                return Err(anyhow::anyhow!("arg 2 is not a float"));
            }
        };
        self.select_tracks(lua, selector)?
            .try_for_each(|t| unsafe {
                if t.is_null() {
                    warn!("track_set_input_gain_fader: track is null");
                    return Ok(());
                }
                invoke::<_, (), _>(
                    &mut *t,
                    "set_input_gain_fader(QVariant)",
                    connection_types::QUEUED_CONNECTION,
                    &(QVariant::from(&gain)),
                )
            })?;
        Ok(mlua::Value::Nil)
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.set_apply_n_cycles(n)
    Set the amount of sync loop cycles future actions will be executed for.
    Setting to 0 will disable this - actions will be open-ended.
    @shoop_lua_fn_docstring.end
    */
    fn set_apply_n_cycles(
        &self,
        _lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("Expected 1 argument, got {}", args.len()));
        }
        let n = match args.get(0).unwrap_or(&mlua::Value::Nil) {
            mlua::Value::Integer(n) => *n,
            _ => {
                return Err(anyhow::anyhow!("arg 1 is not an integer"));
            }
        };
        if self.global_state_registry.is_null() {
            warn!("set_apply_n_cycles: state registry is null");
            return Ok(mlua::Value::Nil);
        }

        unsafe {
            invoke::<_, (), _>(
                &mut *self.global_state_registry,
                "set_apply_n_cycles(QVariant)",
                connection_types::QUEUED_CONNECTION,
                &(QVariant::from(&n)),
            )?;
        };
        Ok(mlua::Value::Nil)
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.get_apply_n_cycles(n)
    Get the amount of sync loop cycles future actions will be executed for.
    0 means disabled.
    @shoop_lua_fn_docstring.end
    */
    fn get_apply_n_cycles(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 0 {
            return Err(anyhow::anyhow!("Expected 0 arguments, got {}", args.len()));
        }
        if self.global_state_registry.is_null() {
            warn!("get_apply_n_cycles: state registry is null");
            return Ok(mlua::Value::Nil);
        }
        unsafe {
            qobject_property_int(&mut *self.global_state_registry, "apply_n_cycles")
                .map(|v| mlua::Value::Integer(v as i64))
                .map_err(|e| anyhow::anyhow!("could not get state registry property: {e}"))
        }
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.set_solo(val)
    Set the global "solo" control state.
    @shoop_lua_fn_docstring.end
    */
    fn set_solo(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("Expected 1 argument, got {}", args.len()));
        }
        let v = match args.get(0).unwrap_or(&mlua::Value::Nil) {
            mlua::Value::Boolean(v) => *v,
            _ => {
                return Err(anyhow::anyhow!("arg 1 is not a bool"));
            }
        };
        if self.global_state_registry.is_null() {
            warn!("set_solo: state registry is null");
            return Ok(mlua::Value::Nil);
        }

        unsafe {
            invoke::<_, (), _>(
                &mut *self.global_state_registry,
                "set_solo_active(QVariant)",
                connection_types::QUEUED_CONNECTION,
                &(QVariant::from(&v)),
            )?;
        };
        Ok(mlua::Value::Nil)
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.get_solo() -> bool
    Get the global "solo" control state.
    @shoop_lua_fn_docstring.end
    */
    fn get_solo(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 0 {
            return Err(anyhow::anyhow!("Expected 0 arguments, got {}", args.len()));
        }
        if self.global_state_registry.is_null() {
            warn!("get_solo: state registry is null");
            return Ok(mlua::Value::Nil);
        }
        unsafe {
            qobject_property_bool(&mut *self.global_state_registry, "solo_active")
                .map(|v| mlua::Value::Boolean(v))
                .map_err(|e| anyhow::anyhow!("could not get state registry property: {e}"))
        }
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.set_sync_active(val)
    Set the global "sync_active" control state.
    @shoop_lua_fn_docstring.end
    */
    fn set_sync_active(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("Expected 1 argument, got {}", args.len()));
        }
        let v = match args.get(0).unwrap_or(&mlua::Value::Nil) {
            mlua::Value::Boolean(v) => *v,
            _ => {
                return Err(anyhow::anyhow!("arg 1 is not a bool"));
            }
        };
        if self.global_state_registry.is_null() {
            warn!("set_sync_active: state registry is null");
            return Ok(mlua::Value::Nil);
        }

        unsafe {
            invoke::<_, (), _>(
                &mut *self.global_state_registry,
                "set_sync_active(QVariant)",
                connection_types::QUEUED_CONNECTION,
                &(QVariant::from(&v)),
            )?;
        };
        Ok(mlua::Value::Nil)
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.get_sync_active() -> bool
    Get the global "sync_active" control state.
    @shoop_lua_fn_docstring.end
    */
    fn get_sync_active(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 0 {
            return Err(anyhow::anyhow!("Expected 0 arguments, got {}", args.len()));
        }
        if self.global_state_registry.is_null() {
            warn!("get_sync_active: state registry is null");
            return Ok(mlua::Value::Nil);
        }
        unsafe {
            qobject_property_bool(&mut *self.global_state_registry, "sync_active")
                .map(|v| mlua::Value::Boolean(v))
                .map_err(|e| anyhow::anyhow!("could not get state registry property: {e}"))
        }
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.set_play_after_record(val)
    Set the global "play_after_record" control state.
    @shoop_lua_fn_docstring.end
    */
    fn set_play_after_record(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("Expected 1 argument, got {}", args.len()));
        }
        let v = match args.get(0).unwrap_or(&mlua::Value::Nil) {
            mlua::Value::Boolean(v) => *v,
            _ => {
                return Err(anyhow::anyhow!("arg 1 is not a bool"));
            }
        };
        if self.global_state_registry.is_null() {
            warn!("set_play_after_record: state registry is null");
            return Ok(mlua::Value::Nil);
        }

        unsafe {
            invoke::<_, (), _>(
                &mut *self.global_state_registry,
                "set_play_after_record_active(QVariant)",
                connection_types::QUEUED_CONNECTION,
                &(QVariant::from(&v)),
            )?;
        };
        Ok(mlua::Value::Nil)
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.get_play_after_record() -> bool
    Get the global "play_after_record" control state.
    @shoop_lua_fn_docstring.end
    */
    fn get_play_after_record(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 0 {
            return Err(anyhow::anyhow!("Expected 0 arguments, got {}", args.len()));
        }
        if self.global_state_registry.is_null() {
            warn!("get_play_after_record: state registry is null");
            return Ok(mlua::Value::Nil);
        }
        unsafe {
            qobject_property_bool(&mut *self.global_state_registry, "play_after_record_active")
                .map(|v| mlua::Value::Boolean(v))
                .map_err(|e| anyhow::anyhow!("could not get state registry property: {e}"))
        }
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.set_default_recording_action(val)
        Set the global "default recording action" control state. Valid values are 'record' or 'grab' - others are ignored.
    @shoop_lua_fn_docstring.end
    */
    fn set_default_recording_action(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("Expected 1 argument, got {}", args.len()));
        }
        let action = match args.get(0).unwrap_or(&mlua::Value::Nil) {
            mlua::Value::String(v) => v.to_string_lossy(),
            _ => {
                return Err(anyhow::anyhow!("arg 1 is not a string"));
            }
        };
        if self.global_state_registry.is_null() {
            warn!("set_default_recording_action: state registry is null");
            return Ok(mlua::Value::Nil);
        }

        unsafe {
            invoke::<_, (), _>(
                &mut *self.global_state_registry,
                "set_default_recording_action(QVariant)",
                connection_types::QUEUED_CONNECTION,
                &(QVariant::from(&QString::from(action))),
            )?;
        };
        Ok(mlua::Value::Nil)
    }

    /*
    @shoop_lua_fn_docstring.start
    shoop_control.get_default_recording_action() -> string
    Get the global "default recording action" control state ('record' or 'grab').
    @shoop_lua_fn_docstring.end
    */
    fn get_default_recording_action(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 0 {
            return Err(anyhow::anyhow!("Expected 0 arguments, got {}", args.len()));
        }
        if self.global_state_registry.is_null() {
            warn!("get_play_after_record: state registry is null");
            return Ok(mlua::Value::Nil);
        }
        unsafe {
            qobject_property_string(&mut *self.global_state_registry, "default_recording_action")
                .map(|v| v.to_string().into_lua(lua).unwrap_or(mlua::Value::Nil))
                .map_err(|e| anyhow::anyhow!("could not get state registry property: {e}"))
        }
    }

    fn select_loop_by_coords(&self, x: i64, y: i64) -> Option<*mut QObject> {
        self.structured_loop_widget_references
            .get(&(x, y))
            .map(|r| unsafe {
                match r.as_ref() {
                    Some(r) => match qpointer_to_qobject(r) {
                        v if v == std::ptr::null_mut() => None,
                        other => Some(other),
                    },
                    None => None,
                }
            })
            .unwrap_or(None)
    }

    fn select_loops_by_coords<'a>(
        &'a self,
        coords: impl Iterator<Item = (i64, i64)> + 'a,
    ) -> impl Iterator<Item = *mut QObject> + 'a {
        {
            coords.map(move |(x, y)| {
                self.select_loop_by_coords(x, y)
                    .unwrap_or(std::ptr::null_mut())
            })
        }
    }

    fn all_loops_iter(
        &self,
    ) -> Result<impl Iterator<Item = *mut QObject> + use<'_>, anyhow::Error> {
        Ok(self
            .structured_loop_widget_references
            .values()
            .map(|qpointer| unsafe { qpointer_to_qobject(&qpointer) })
            .filter(|p| !p.is_null()))
    }

    fn select_loops<'a>(
        &'a self,
        lua: &mlua::Lua,
        selector: &mlua::Value,
    ) -> Result<impl Iterator<Item = *mut QObject> + 'a, anyhow::Error> {
        match selector {
            mlua::Value::Table(_) => {
                if let Some(coords) = as_coords(selector) {
                    Ok(Either::Left(Either::Left(
                        self.select_loops_by_coords(std::iter::once(coords)),
                    )))
                } else if let Some(multi_coords) = as_multi_coords(selector) {
                    Ok(Either::Left(Either::Right(
                        self.select_loops_by_coords(multi_coords.into_iter()),
                    )))
                } else {
                    Err(anyhow::anyhow!("Unsupported loop selector: table cannot be interpreted as (list of) coordinates"))
                }
            }
            mlua::Value::Nil => Ok(Either::Right(std::iter::empty())),
            others => Err(anyhow::anyhow!(
                "Unsupported loop selector type: {others:?}"
            )),
        }
    }

    fn select_track_by_index(&self, idx: i64) -> Option<*mut QObject> {
        self.structured_track_control_widget_references
            .get(&idx)
            .map(|r| unsafe {
                match r.as_ref() {
                    Some(r) => match qpointer_to_qobject(r) {
                        v if v == std::ptr::null_mut() => None,
                        other => Some(other),
                    },
                    None => None,
                }
            })
            .unwrap_or(None)
    }

    fn select_tracks_by_indices<'a>(
        &'a self,
        indices: impl Iterator<Item = i64> + 'a,
    ) -> impl Iterator<Item = *mut QObject> + 'a {
        {
            indices.map(move |idx| {
                self.select_track_by_index(idx)
                    .unwrap_or(std::ptr::null_mut())
            })
        }
    }

    fn select_tracks<'a>(
        &'a self,
        lua: &mlua::Lua,
        selector: &mlua::Value,
    ) -> Result<impl Iterator<Item = *mut QObject> + 'a, anyhow::Error> {
        match selector {
            mlua::Value::Integer(idx) => Ok(Either::Left(Either::Left(
                self.select_tracks_by_indices(std::iter::once(*idx)),
            ))),
            mlua::Value::Table(table) => {
                if let Some(indices) = as_track_indices(selector) {
                    Ok(Either::Left(Either::Right(
                        self.select_tracks_by_indices(indices.into_iter()),
                    )))
                } else {
                    Err(anyhow::anyhow!("Unsupported track selector: table cannot be interpreted as list of indices"))
                }
            }
            mlua::Value::Nil => Ok(Either::Right(std::iter::empty())),
            others => Err(anyhow::anyhow!(
                "Unsupported track selector type: {others:?}"
            )),
        }
    }
}

impl SessionControlHandler {
    pub fn initialize_impl(mut self: Pin<&mut SessionControlHandler>) {
        unsafe {
            let self_qobj = self.as_mut().pin_mut_qobject_ptr();
            connect_or_report(
                &*self_qobj,
                "loop_widget_referencesChanged()",
                &*self_qobj,
                "update_structured_loop_widget_references()",
                connection_types::AUTO_CONNECTION,
            );
            connect_or_report(
                &*self_qobj,
                "track_control_widget_referencesChanged()",
                &*self_qobj,
                "update_structured_track_control_widget_references()",
                connection_types::AUTO_CONNECTION,
            );
        }
    }

    pub fn install_on_lua_engine(self: Pin<&mut SessionControlHandler>, engine: *mut QObject) {
        self.lua_target.borrow_mut().install_on_lua_engine(engine);
    }

    pub fn update_structured_track_control_widget_references(
        mut self: Pin<&mut SessionControlHandler>,
    ) {
        if let Err(e) = || -> Result<(), anyhow::Error> {
            let mut result: BTreeMap<i64, cxx::UniquePtr<QPointerQObject>> = BTreeMap::default();

            let self_qobj: *mut QObject = unsafe { self.as_mut().pin_mut_qobject_ptr() };

            for track_variant in self.track_control_widget_references.iter() {
                let track_qobj = qvariant_to_qobject_ptr(&track_variant)?;
                if track_qobj.is_null() {
                    continue;
                }
                let track_idx = unsafe { qobject_property_int(&*track_qobj, "track_idx")? };
                result.insert(track_idx as i64, unsafe {
                    qpointer_from_qobject(track_qobj)
                });

                unsafe {
                    let _ = connect(
                        &*track_qobj,
                        "track_idxChanged()",
                        &*self_qobj,
                        "update_structured_track_control_widget_references()",
                        connection_types::AUTO_CONNECTION | connection_types::UNIQUE_CONNECTION,
                    );
                }
            }

            trace!("track references: {:?}", result.keys());
            self.lua_target
                .borrow_mut()
                .structured_track_control_widget_references = result;

            Ok(())
        }() {
            error!("Could not set track references: {e}");
        }
    }

    pub fn update_structured_loop_widget_references(mut self: Pin<&mut SessionControlHandler>) {
        if let Err(e) = || -> Result<(), anyhow::Error> {
            let mut result: BTreeMap<(i64, i64), cxx::UniquePtr<QPointerQObject>> =
                BTreeMap::default();

            let self_qobj: *mut QObject = unsafe { self.as_mut().pin_mut_qobject_ptr() };

            for loop_qobj in self.loop_widget_references.iter() {
                let loop_qobj = qvariant_to_qobject_ptr(&loop_qobj)?;
                if loop_qobj.is_null() {
                    continue;
                }
                let track_idx = unsafe { qobject_property_int(&*loop_qobj, "track_idx")? };
                let idx_in_track = unsafe { qobject_property_int(&*loop_qobj, "idx_in_track")? };
                result.insert((track_idx as i64, idx_in_track as i64), unsafe {
                    qpointer_from_qobject(loop_qobj)
                });

                unsafe {
                    let _ = connect(
                        &*loop_qobj,
                        "track_idxChanged()",
                        &*self_qobj,
                        "update_structured_loop_widget_references()",
                        connection_types::AUTO_CONNECTION | connection_types::UNIQUE_CONNECTION,
                    );
                    let _ = connect(
                        &*loop_qobj,
                        "idx_in_trackChanged()",
                        &*self_qobj,
                        "update_structured_loop_widget_references()",
                        connection_types::AUTO_CONNECTION | connection_types::UNIQUE_CONNECTION,
                    );
                }
            }

            trace!("loop references: {:?}", result.keys());
            self.lua_target
                .borrow_mut()
                .structured_loop_widget_references = result;

            Ok(())
        }() {
            error!("Could not set loop references: {e}");
        }
    }

    pub fn set_selected_loops(self: Pin<&mut SessionControlHandler>, loops: QList_QVariant) {
        let mut converted: Vec<*mut QObject> = Vec::default();
        for variant in loops.iter() {
            let qobj = qvariant_to_qobject_ptr(variant).unwrap_or(std::ptr::null_mut());
            if !qobj.is_null() {
                converted.push(qobj);
            }
        }
        if self.lua_target.borrow().selected_loops != converted {
            trace!("selected loops: {:?}", converted);
            self.lua_target.borrow_mut().selected_loops = converted;
            self.selected_loops_changed();
        }
    }

    pub fn set_targeted_loop(self: Pin<&mut SessionControlHandler>, maybe_loop: QVariant) {
        let qobj = qvariant_to_qobject_ptr(&maybe_loop).unwrap_or(std::ptr::null_mut());
        let value = match qobj.is_null() {
            true => None,
            false => Some(qobj),
        };
        if self.lua_target.borrow().maybe_targeted_loop != value {
            trace!("targeted loop: {:?}", value);
            self.lua_target.borrow_mut().maybe_targeted_loop = value;
            self.targeted_loop_changed();
        }
    }

    pub fn get_selected_loops(self: Pin<&mut SessionControlHandler>) -> QList_QVariant {
        let mut rval: QList<QVariant> = QList::default();
        for qobj in self.lua_target.borrow().selected_loops.iter() {
            rval.append(qobject_ptr_to_qvariant(qobj).unwrap_or(QVariant::default()));
        }
        rval
    }

    pub fn get_targeted_loop(self: Pin<&mut SessionControlHandler>) -> QVariant {
        qobject_ptr_to_qvariant(
            &self
                .lua_target
                .borrow()
                .maybe_targeted_loop
                .unwrap_or(std::ptr::null_mut()),
        )
        .unwrap_or(QVariant::default())
    }

    pub fn set_session(self: Pin<&mut SessionControlHandler>, session: *mut QObject) {
        if self.lua_target.borrow().session != session {
            self.lua_target.borrow_mut().session = session;
            self.session_changed();
        }
    }

    pub fn get_session(self: Pin<&mut SessionControlHandler>) -> *mut QObject {
        self.lua_target.borrow().session
    }

    pub fn set_global_state_registry(
        self: Pin<&mut SessionControlHandler>,
        registry: *mut QObject,
    ) {
        let current = self.lua_target.borrow().global_state_registry;
        if (current != registry) {
            self.lua_target.borrow_mut().global_state_registry = registry;
            self.global_state_registry_changed();
        }
    }

    pub fn get_global_state_registry(self: Pin<&mut SessionControlHandler>) -> *mut QObject {
        self.lua_target.borrow_mut().global_state_registry
    }
}

pub fn register_qml_type(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe {
        register_qml_type_session_control_handler(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp);
    }
}
