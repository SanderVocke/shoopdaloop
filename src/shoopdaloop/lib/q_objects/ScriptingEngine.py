from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer

import lupa
import copy

from ..logging import Logger

import os
import inspect
script_pwd = os.path.dirname(__file__)
lua_scriptdir = script_pwd + '/../lua'

# A scripting context can be provided to the scripting engine
# for running any command or script. It provides the following
# context for the script to run with:
# - The script can use declare_context("varname") to define variables
#   which will be visible to any other script(s) run with this context,
#   but not to scripts which don't have this context.
class ScriptingContext(QObject):
    def __init__(self, engine, parent=None):
        super(ScriptingContext, self).__init__(parent)
        self.context_id = engine.eval('return __shoop_new_context()')

    def use(self, engine):
        engine.execute('__shoop_use_context({})'.format(self.context_id))

class ScriptingNullContext(QObject):
    def __init__(self, parent=None):
        super(ScriptingNullContext, self).__init__(parent)

    def use(self, engine):
        engine.execute('__shoop_use_context(nil)')

class ScriptingEngine(QObject):
    def __init__(self, parent=None):
        super(ScriptingEngine, self).__init__(parent)
        self.logger = Logger('Frontend.ScriptingEngine')
        self.logger.debug('Initializing Lua runtime.')
        self.lua = lupa.LuaRuntime()
        self._current_context = ScriptingNullContext()
        registrar = self.lua.eval('function(name, fn) _G[name] = fn end')
        registrar('__shoop_print', lambda s, l=self.logger: l.info('Lua Script: {}'.format(s)))
        self.execute_builtin_script('sandbox.lua', False)
        self.run_sandboxed = self.lua.eval('function (code) return __shoop_run_sandboxed(code) end')
        self.execute_builtin_script('runtime_init.lua')
        self.define_callbacks()
        

    def define_global_callback(self, py_cb, lua_name, overwrite=True):
        self.logger.debug('Declaring global callback "{}"'.format(lua_name))
        sig = inspect.signature(py_cb)
        declaration_name = ('declare_global' if overwrite else 'declare_new_global')
        # create a temporary registrar which can be used to register our global function
        registrar = self.eval('return function(py_cb, name) {0}(name, function({1}) return py_cb({1}) end) end'.format(
            declaration_name,
            ','.join(sig.parameters.keys())
        ))
        registrar(py_cb, lua_name)

    def define_callbacks(self):
        pass

    def execute_builtin_script(self, filename, sandboxed=True):
        self.logger.debug('Running built-in script: {}'.format(filename))
        script = None
        with open(lua_scriptdir  + '/' + filename, 'r') as f:
            script = f.read()
        return self.execute(script, None, sandboxed)

    ######################
    # PROPERTIES
    ######################

    ###########
    ## SLOTS
    ###########

    @Slot('QVariant')
    def use_context(self, context):
        self.logger.debug("Using context: {}".format(context))
        context.use(self)
        self._current_context = context
    
    @Slot(result='QVariant')
    def current_context(self):
        return self._current_context
    
    @Slot(result='QVariant')
    def new_context(self):
        return ScriptingContext(self)

    @Slot(str, 'QVariant', bool, result='QVariant')
    def eval(self, lua_code, context=None, sandboxed=True):
        self.logger.trace('Eval (context {}):\n{}'.format(context, lua_code))
        prev_context = self.current_context()
        if context:
            self.use_context(context)
        if not sandboxed:
            rval = self.lua.eval(lua_code)
        else:
            rval = self.run_sandboxed(lua_code)
        if context:
            self.use_context(prev_context)
        return rval
    
    @Slot(str, 'QVariant')
    def execute(self, lua_code, context=None, sandboxed=True):
        self.logger.trace('Execute (context {}):\n{}'.format(context, lua_code))
        prev_context = self.current_context()
        if context:
            self.use_context(context)
        if not sandboxed:
            self.lua.execute(lua_code)
        else:
            self.run_sandboxed(lua_code)
        if context:
            self.use_context(prev_context)

    ##########
    ## INTERNAL MEMBERS
    ##########
