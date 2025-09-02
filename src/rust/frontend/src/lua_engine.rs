use std::{
    cell::RefCell,
    collections::HashMap,
    rc::{Rc, Weak},
};

use common::logging::macros::*;
use mlua;

use crate::lua_callback::LuaCallback;
shoop_log_unit!("Frontend.LuaEngine");

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum LuaScope {
    Sandboxed,
    Global,
}

pub struct LuaEngineImpl {
    weak_self: Weak<RefCell<LuaEngineImpl>>,
    pub lua: mlua::Lua,
    preloaded_libs: HashMap<String, String>,
    run_sandboxed: Option<mlua::Function>,
    require: Option<mlua::Function>,
    get_builtin_script_fn: Option<Box<dyn Fn(&str) -> Result<String, anyhow::Error>>>,
}

pub struct LuaEngine {
    pub lua: Rc<RefCell<LuaEngineImpl>>,
}

impl Default for LuaEngine {
    fn default() -> Self {
        let rval = Self {
            lua: Rc::new(RefCell::new(LuaEngineImpl {
                weak_self: Weak::new(),
                lua: mlua::Lua::new(),
                preloaded_libs: HashMap::default(),
                run_sandboxed: None,
                require: None,
                get_builtin_script_fn: None,
            })),
        };
        let weak = Rc::downgrade(&rval.lua);
        rval.lua.borrow_mut().weak_self = weak;
        rval
    }
}

impl LuaEngineImpl {
    fn register_print_functions(&mut self) -> Result<(), anyhow::Error> {
        let globals = self.lua.globals();

        globals
            .set(
                "__shoop_print",
                self.lua
                    .create_function(|_, (msg,): (String,)| {
                        info!("{msg}");
                        Ok(())
                    })
                    .map_err(|e| anyhow::anyhow!("Failed to create print function: {e}"))?,
            )
            .map_err(|e| anyhow::anyhow!("Failed to register print function: {e}"))?;
        globals
            .set(
                "__shoop_print_trace",
                self.lua
                    .create_function(|_, (msg,): (String,)| {
                        trace!("{msg}");
                        Ok(())
                    })
                    .map_err(|e| anyhow::anyhow!("Failed to create print function: {e}"))?,
            )
            .map_err(|e| anyhow::anyhow!("Failed to register print function: {e}"))?;
        globals
            .set(
                "__shoop_print_debug",
                self.lua
                    .create_function(|_, (msg,): (String,)| {
                        debug!("{msg}");
                        Ok(())
                    })
                    .map_err(|e| anyhow::anyhow!("Failed to create print function: {e}"))?,
            )
            .map_err(|e| anyhow::anyhow!("Failed to register print function: {e}"))?;
        globals
            .set(
                "__shoop_print_info",
                self.lua
                    .create_function(|_, (msg,): (String,)| {
                        info!("{msg}");
                        Ok(())
                    })
                    .map_err(|e| anyhow::anyhow!("Failed to create print function: {e}"))?,
            )
            .map_err(|e| anyhow::anyhow!("Failed to register print function: {e}"))?;
        globals
            .set(
                "__shoop_print_warning",
                self.lua
                    .create_function(|_, (msg,): (String,)| {
                        warn!("{msg}");
                        Ok(())
                    })
                    .map_err(|e| anyhow::anyhow!("Failed to create print function: {e}"))?,
            )
            .map_err(|e| anyhow::anyhow!("Failed to register print function: {e}"))?;
        globals
            .set(
                "__shoop_print_error",
                self.lua
                    .create_function(|_, (msg,): (String,)| {
                        error!("{msg}");
                        Ok(())
                    })
                    .map_err(|e| anyhow::anyhow!("Failed to create print function: {e}"))?,
            )
            .map_err(|e| anyhow::anyhow!("Failed to register print function: {e}"))?;

        Ok(())
    }

    pub fn execute_builtin_script(
        &mut self,
        script_subpath: &str,
        sandboxed: bool,
    ) -> Result<(), anyhow::Error> {
        debug!("Running built-in script: {script_subpath}");
        let content = self
            .get_builtin_script_fn
            .as_ref()
            .ok_or(anyhow::anyhow!("no builtin script fn registered"))?(
            script_subpath
        )?;
        self.execute(content.as_str(), Some(script_subpath), sandboxed)?;
        Ok(())
    }

    pub fn prepare_sandbox(&mut self) -> Result<(), anyhow::Error> {
        let sandbox_script: &'static str = include_str!("../../../lua/system/sandbox.lua");
        self.execute(sandbox_script, Some("sandbox.lua"), false)?;
        {
            let globals = self.lua.globals();
            self.run_sandboxed = Some(
                globals
                    .get("__shoop_run_sandboxed")
                    .map_err(|e| anyhow::anyhow!("failed to get __shoop_run_sandboxed: {e}"))?,
            );

            let weak_self = self.weak_self.clone();
            self.require = Some(
                self.lua
                    .create_function(move |_, (name,): (String,)| {
                        match || -> Result<mlua::Value, anyhow::Error> {
                            let strong_self = weak_self
                                .upgrade()
                                .ok_or(anyhow::anyhow!("Lua went out of scope"))?;
                            let strong_self = strong_self.borrow();
                            let lib_content = strong_self
                                .preloaded_libs
                                .get(&name)
                                .ok_or(anyhow::anyhow!("Lib not found: {name}"))?;
                            strong_self
                                .evaluate(&lib_content, Some(&name), true)
                                .map_err(|e| anyhow::anyhow!("Could not import lib {name}: {e}"))
                        }() {
                            Ok(result) => Ok(result),
                            Err(e) => {
                                error!("Could not require lib '{name}': {e}");
                                Ok(mlua::Value::default())
                            }
                        }
                    })
                    .map_err(|e| anyhow::anyhow!("Could not create require function: {e}"))?,
            );
        }
        let require = self.require.clone();
        self.set_toplevel("require", require)?;
        Ok(())
    }

    pub fn require(&self, name: &str) -> Result<mlua::Value, anyhow::Error> {
        if name == "shoop_control" {
            // Special built-in library which is always loaded
            return self
                .lua
                .load("__shoop_control_interface")
                .set_name("get control interface")
                .eval::<mlua::Value>()
                .map_err(|e| anyhow::anyhow!("could not get control interface: {e}"));
        }
        if !self.preloaded_libs.contains_key(name) {
            return Err(anyhow::anyhow!("Cannot require unloaded library: {name}"));
        }
        self.lua
            .load(
                self.preloaded_libs
                    .get(name)
                    .ok_or(anyhow::anyhow!("No such builtin script: {name}"))?,
            )
            .eval::<mlua::Value>()
            .map_err(|e| anyhow::anyhow!("could not evaluate required builtin script {name}: {e}"))
    }

    pub fn evaluate<R>(
        &self,
        code: &str,
        script_name: Option<&str>,
        sandboxed: bool,
    ) -> Result<R, anyhow::Error>
    where
        R: mlua::FromLuaMulti,
    {
        trace!(
            "evaluate (sandboxed: {sandboxed}, name: {}):\n{code}",
            script_name.unwrap_or("(none)")
        );
        match sandboxed {
            true => self
                .run_sandboxed
                .as_ref()
                .ok_or(anyhow::anyhow!("no sandbox function set"))?
                .call::<R>((code,))
                .map_err(|e| anyhow::anyhow!("Could not evaluate sandboxed: {e}")),
            false => self
                .lua
                .load(code)
                .eval::<R>()
                .map_err(|e| anyhow::anyhow!("Could not evaluate unsandboxed: {e}")),
        }
    }

    pub fn execute(
        &self,
        code: &str,
        script_name: Option<&str>,
        sandboxed: bool,
    ) -> Result<(), anyhow::Error> {
        self.evaluate::<()>(code, script_name, sandboxed)
    }

    pub fn set_toplevel(
        &mut self,
        key: &str,
        value: impl mlua::IntoLua,
    ) -> Result<(), anyhow::Error> {
        self.evaluate::<mlua::Function>(
            &format!("return function(value) {} = value end", key),
            Some("registrar"),
            true,
        )
        .map_err(|e| anyhow::anyhow!("Could not create registrar: {e}"))?
        .call((value,))
        .map_err(|e| anyhow::anyhow!("Could not set {key}: {e}"))
    }

    pub fn initialize(
        &mut self,
        get_builtin_script_fn: impl Fn(&str) -> Result<String, anyhow::Error> + 'static,
        builtin_libs: HashMap<String, String>,
    ) -> Result<(), anyhow::Error> {
        debug!("Initializing engine");

        self.preloaded_libs = builtin_libs;
        self.get_builtin_script_fn = Some(Box::new(get_builtin_script_fn));

        self.register_print_functions()?;
        self.prepare_sandbox()?;

        Ok(())
    }

    fn create_callback_fn(
        &mut self,
        name: &str,
        callback: &Rc<Box<dyn LuaCallback>>,
    ) -> Result<mlua::Function, anyhow::Error> {
        let weak = Rc::downgrade(callback);
        let name = name.to_string();
        let weak_self = self.weak_self.clone();
        self.lua
            .create_function(
                move |_, (multi,): (mlua::MultiValue,)| -> Result<mlua::Value, mlua::Error> {
                    match || -> Result<mlua::Value, anyhow::Error> {
                        let strong_cb = weak
                            .upgrade()
                            .ok_or(anyhow::anyhow!("callback went out of scope"))?;
                        let strong_self = weak_self
                            .upgrade()
                            .ok_or(anyhow::anyhow!("lua went out of scope"))?;
                        let strong_self = strong_self.borrow();
                        strong_cb.call(&strong_self.lua, multi)
                    }() {
                        Ok(result) => Ok(result),
                        Err(e) => {
                            error!("Could not call callback '{}': {e}", name);
                            Ok(mlua::Value::default())
                        }
                    }
                },
            )
            .map_err(|e| anyhow::anyhow!("Could not create callback function: {e}"))
    }

    pub fn register_callback(
        &mut self,
        name: &str,
        scope: LuaScope,
        callback: &Rc<Box<dyn LuaCallback>>,
    ) -> Result<(), anyhow::Error> {
        let call = self.create_callback_fn(name, callback)?;
        match scope {
            LuaScope::Global => {
                self.lua
                    .globals()
                    .set(name, call)
                    .map_err(|e| anyhow::anyhow!("Unable to set global callback: {e}"))?;
            }
            LuaScope::Sandboxed => {
                self.set_toplevel(name, call)?;
            }
        }
        Ok(())
    }

    pub fn register_callbacks_module<'a>(
        &mut self,
        name: &str,
        scope: LuaScope,
        callbacks: impl Iterator<Item = (&'a str, &'a Rc<Box<dyn LuaCallback>>)>,
    ) -> Result<(), anyhow::Error> {
        let module = self
            .lua
            .create_table()
            .map_err(|e| anyhow::anyhow!("Could not create table: {e}"))?;

        for (name, callback) in callbacks {
            let callback = self.create_callback_fn(name, callback)?;
            module
                .set(name, callback)
                .map_err(|e| anyhow::anyhow!("Could not set module callback {name}: {e}"))?;
        }

        match scope {
            LuaScope::Global => {
                self.lua
                    .globals()
                    .set(name, module)
                    .map_err(|e| anyhow::anyhow!("Unable to set global callback: {e}"))?;
            }
            LuaScope::Sandboxed => {
                self.set_toplevel(name, module)?;
            }
        }
        Ok(())
    }
}

impl LuaEngine {
    pub fn execute_builtin_script(
        &mut self,
        script_subpath: &str,
        sandboxed: bool,
    ) -> Result<(), anyhow::Error> {
        self.lua
            .borrow_mut()
            .execute_builtin_script(script_subpath, sandboxed)
    }

    pub fn require(&self, name: &str) -> Result<mlua::Value, anyhow::Error> {
        self.lua.borrow().require(name)
    }

    pub fn evaluate<R>(
        &self,
        code: &str,
        script_name: Option<&str>,
        sandboxed: bool,
    ) -> Result<R, anyhow::Error>
    where
        R: mlua::FromLuaMulti,
    {
        self.lua.borrow().evaluate(code, script_name, sandboxed)
    }

    pub fn execute(
        &self,
        code: &str,
        script_name: Option<&str>,
        sandboxed: bool,
    ) -> Result<(), anyhow::Error> {
        self.lua.borrow().execute(code, script_name, sandboxed)
    }

    pub fn set_toplevel(
        &mut self,
        key: &str,
        value: impl mlua::IntoLua,
    ) -> Result<(), anyhow::Error> {
        self.lua.borrow_mut().set_toplevel(key, value)
    }

    pub fn initialize(
        &mut self,
        get_builtin_script_fn: impl Fn(&str) -> Result<String, anyhow::Error> + 'static,
        builtin_libs: HashMap<String, String>,
    ) -> Result<(), anyhow::Error> {
        self.lua
            .borrow_mut()
            .initialize(get_builtin_script_fn, builtin_libs)
    }

    pub fn register_callback(
        &mut self,
        name: &str,
        scope: LuaScope,
        callback: &Rc<Box<dyn LuaCallback>>,
    ) -> Result<(), anyhow::Error> {
        self.lua
            .borrow_mut()
            .register_callback(name, scope, callback)
    }

    pub fn register_callbacks_module<'a>(
        &mut self,
        name: &str,
        scope: LuaScope,
        callbacks: impl Iterator<Item = (&'a str, &'a Rc<Box<dyn LuaCallback>>)>,
    ) -> Result<(), anyhow::Error> {
        self.lua
            .borrow_mut()
            .register_callbacks_module(name, scope, callbacks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO_WORLD_IMPL: &'static str = r#"
function test_hello_world()
  return 'hello world'
end"#;

    fn testing_builtins(name: &str) -> Result<String, anyhow::Error> {
        match name {
            "test_script_hello_world" => Ok(HELLO_WORLD_IMPL.to_string()),
            other => Err(anyhow::anyhow!("Not a testing builtin: {other}")),
        }
    }

    #[test]
    fn test_basic_eval_expression_unsandboxed() {
        let mut eng = LuaEngine::default();
        eng.initialize(|_| Err(anyhow::anyhow!("n/a")), HashMap::default())
            .unwrap();
        assert_eq!(eng.evaluate::<i32>("1 + 1", None, false).unwrap(), 2);
    }

    #[test]
    fn test_basic_eval_chunk_unsandboxed() {
        let mut eng = LuaEngine::default();
        eng.initialize(|_| Err(anyhow::anyhow!("n/a")), HashMap::default())
            .unwrap();
        assert_eq!(eng.evaluate::<i32>("return 1 + 1", None, false).unwrap(), 2);
    }

    #[test]
    fn test_basic_eval_chunk_sandboxed() {
        let mut eng = LuaEngine::default();
        eng.initialize(|_| Err(anyhow::anyhow!("n/a")), HashMap::default())
            .unwrap();
        assert_eq!(eng.evaluate::<i32>("return 1 + 1", None, true).unwrap(), 2);
    }

    #[test]
    fn test_builtin_lib_sandboxed() {
        let mut eng = LuaEngine::default();
        let mut libs: HashMap<String, String> = HashMap::default();
        libs.insert(
            "test_hello_world_lib".to_string(),
            HELLO_WORLD_IMPL.to_string(),
        );
        eng.initialize(|_| Err(anyhow::anyhow!("n/a")), libs)
            .unwrap();
        assert_eq!(
            eng.evaluate::<String>(
                r#"
require('test_hello_world_lib')
return test_hello_world()
"#,
                None,
                true
            )
            .unwrap(),
            "hello world"
        );
    }

    #[test]
    fn test_builtin_script_sandboxed() {
        let mut eng = LuaEngine::default();
        eng.initialize(testing_builtins, HashMap::default())
            .unwrap();
        eng.execute_builtin_script("test_script_hello_world", true)
            .unwrap();
        assert_eq!(
            eng.evaluate::<String>("return test_hello_world()", None, true)
                .unwrap(),
            "hello world"
        );
    }

    #[test]
    fn test_callback_sandboxed() {
        struct TestCallback {}
        impl LuaCallback for TestCallback {
            fn call(
                &self,
                _: &mlua::Lua,
                args: mlua::MultiValue,
            ) -> Result<mlua::Value, anyhow::Error> {
                if args.len() != 2 {
                    return Err(anyhow::anyhow!("Incorrect amount of args"));
                }
                let a = args.front().unwrap().as_i64().unwrap();
                let b = args.back().unwrap().as_i64().unwrap();
                Ok(mlua::Value::Integer(a + b))
            }
        }

        let cb: Rc<Box<dyn LuaCallback>> = Rc::new(Box::new(TestCallback {}));

        let mut eng = LuaEngine::default();
        eng.initialize(testing_builtins, HashMap::default())
            .unwrap();
        eng.register_callback("my_test_add", LuaScope::Sandboxed, &cb)
            .unwrap();

        assert_eq!(
            eng.evaluate::<i64>("return my_test_add(1, 2)", None, true)
                .unwrap(),
            3
        );
    }

    #[test]
    fn test_callback_unsandboxed() {
        struct TestCallback {}
        impl LuaCallback for TestCallback {
            fn call(
                &self,
                _: &mlua::Lua,
                args: mlua::MultiValue,
            ) -> Result<mlua::Value, anyhow::Error> {
                if args.len() != 2 {
                    return Err(anyhow::anyhow!("Incorrect amount of args"));
                }
                let a = args.front().unwrap().as_i64().unwrap();
                let b = args.back().unwrap().as_i64().unwrap();
                Ok(mlua::Value::Integer(a + b))
            }
        }

        let cb: Rc<Box<dyn LuaCallback>> = Rc::new(Box::new(TestCallback {}));

        let mut eng = LuaEngine::default();
        eng.initialize(testing_builtins, HashMap::default())
            .unwrap();
        eng.register_callback("my_test_add", LuaScope::Global, &cb)
            .unwrap();

        assert_eq!(
            eng.evaluate::<i64>("return my_test_add(1, 2)", None, false)
                .unwrap(),
            3
        );
    }

    #[test]
    fn test_callbacks_module_sandboxed() {
        struct TestAddCallback {}
        impl LuaCallback for TestAddCallback {
            fn call(
                &self,
                _: &mlua::Lua,
                args: mlua::MultiValue,
            ) -> Result<mlua::Value, anyhow::Error> {
                if args.len() != 2 {
                    return Err(anyhow::anyhow!("Incorrect amount of args"));
                }
                let a = args.front().unwrap().as_i64().unwrap();
                let b = args.back().unwrap().as_i64().unwrap();
                Ok(mlua::Value::Integer(a + b))
            }
        }
        struct TestSubtractCallback {}
        impl LuaCallback for TestSubtractCallback {
            fn call(
                &self,
                _: &mlua::Lua,
                args: mlua::MultiValue,
            ) -> Result<mlua::Value, anyhow::Error> {
                if args.len() != 2 {
                    return Err(anyhow::anyhow!("Incorrect amount of args"));
                }
                let a = args.front().unwrap().as_i64().unwrap();
                let b = args.back().unwrap().as_i64().unwrap();
                Ok(mlua::Value::Integer(a - b))
            }
        }

        let add_cb: Rc<Box<dyn LuaCallback>> = Rc::new(Box::new(TestAddCallback {}));
        let sub_cb: Rc<Box<dyn LuaCallback>> = Rc::new(Box::new(TestSubtractCallback {}));

        let mut module: HashMap<String, Rc<Box<dyn LuaCallback>>> = HashMap::new();
        module.insert("add".to_string(), add_cb);
        module.insert("sub".to_string(), sub_cb);

        let mut eng = LuaEngine::default();
        eng.initialize(testing_builtins, HashMap::default())
            .unwrap();
        eng.register_callbacks_module(
            "testmodule",
            LuaScope::Sandboxed,
            module.iter().map(|(name, cb)| (name.as_str(), cb)),
        )
        .unwrap();

        assert_eq!(
            eng.evaluate::<i64>("return testmodule.add(1, 2)", None, true)
                .unwrap(),
            3
        );
        assert_eq!(
            eng.evaluate::<i64>("return testmodule.sub(1, 2)", None, true)
                .unwrap(),
            -1
        );
    }

    #[test]
    fn test_callbacks_module_unsandboxed() {
        struct TestAddCallback {}
        impl LuaCallback for TestAddCallback {
            fn call(
                &self,
                _: &mlua::Lua,
                args: mlua::MultiValue,
            ) -> Result<mlua::Value, anyhow::Error> {
                if args.len() != 2 {
                    return Err(anyhow::anyhow!("Incorrect amount of args"));
                }
                let a = args.front().unwrap().as_i64().unwrap();
                let b = args.back().unwrap().as_i64().unwrap();
                Ok(mlua::Value::Integer(a + b))
            }
        }
        struct TestSubtractCallback {}
        impl LuaCallback for TestSubtractCallback {
            fn call(
                &self,
                _: &mlua::Lua,
                args: mlua::MultiValue,
            ) -> Result<mlua::Value, anyhow::Error> {
                if args.len() != 2 {
                    return Err(anyhow::anyhow!("Incorrect amount of args"));
                }
                let a = args.front().unwrap().as_i64().unwrap();
                let b = args.back().unwrap().as_i64().unwrap();
                Ok(mlua::Value::Integer(a - b))
            }
        }

        let add_cb: Rc<Box<dyn LuaCallback>> = Rc::new(Box::new(TestAddCallback {}));
        let sub_cb: Rc<Box<dyn LuaCallback>> = Rc::new(Box::new(TestSubtractCallback {}));

        let mut module: HashMap<String, Rc<Box<dyn LuaCallback>>> = HashMap::new();
        module.insert("add".to_string(), add_cb);
        module.insert("sub".to_string(), sub_cb);

        let mut eng = LuaEngine::default();
        eng.initialize(testing_builtins, HashMap::default())
            .unwrap();
        eng.register_callbacks_module(
            "testmodule",
            LuaScope::Global,
            module.iter().map(|(name, cb)| (name.as_str(), cb)),
        )
        .unwrap();

        assert_eq!(
            eng.evaluate::<i64>("return testmodule.add(1, 2)", None, false)
                .unwrap(),
            3
        );
        assert_eq!(
            eng.evaluate::<i64>("return testmodule.sub(1, 2)", None, false)
                .unwrap(),
            -1
        );
    }
}
