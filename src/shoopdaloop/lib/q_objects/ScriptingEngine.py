from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQml import QJSValue

import lupa
import copy
import traceback

from ..logging import Logger
from ..lua_qobject_interface import create_lua_qobject_interface

import os
import inspect
script_pwd = os.path.dirname(__file__)
lua_scriptdir = script_pwd + '/../lua'

class ScriptExecutionError(Exception):
    pass

# A scripting context can be provided to the scripting engine
# for running any command or script. It provides the following
# context for the script to run with:
# - The script can use declare_context("varname") to define variables
#   which will be visible to any other script(s) run with this context,
#   but not to scripts which don't have this context.
class ScriptingEngine(QObject):
    def __init__(self, parent=None):
        super(ScriptingEngine, self).__init__(parent)
        self.logger = Logger('Frontend.ScriptingEngine')
        self.lua_logger = Logger('Frontend.LuaScript')
        self.logger.debug(lambda: 'Initializing Lua runtime.')
        self.lua = lupa.LuaRuntime()
        self._current_context_id = None
        self.global_registrar = self.lua.eval('function(name, fn) _G[name] = fn end')
        self.global_registrar('__shoop_print', lambda s, l=self.lua_logger: l.info('{}'.format(s)))
        self.global_registrar('__shoop_print_debug', lambda s, l=self.lua_logger: l.debug('{}'.format(s)))
        self.global_registrar('__shoop_print_info', lambda s, l=self.lua_logger: l.info('{}'.format(s)))
        self.global_registrar('__shoop_print_warning', lambda s, l=self.lua_logger: l.warning('{}'.format(s)))
        self.global_registrar('__shoop_print_error', lambda s, l=self.lua_logger: l.error('{}'.format(s)))
        self.execute_builtin_script('system/sandbox.lua', False)
        self.run_sandboxed = self.lua.eval('function (code) return __shoop_run_sandboxed(code) end')
        self.execute('package.path = package.path .. ";{}"'.format(lua_scriptdir + '/lib/?.lua'), None, None, True, False)
        self.py_list_to_lua_table = self.lua.eval('''
        function(val)
            local t = {}
            for index, value in python.enumerate(val) do
                t[index + 1] = value
            end
            return t
        end
        ''')
        self.py_dict_to_lua_table = self.lua.eval('''
        function(val)
            local t = {}
            for key, value in python.iterex(val.items()) do
                t[key] = value
            end
            return t
        end
        ''')
        self.execute_builtin_script('system/runtime_init.lua', True)

    def execute_builtin_script(self, filename, sandboxed=True):
        self.logger.debug(lambda: 'Running built-in script: {}'.format(filename))
        script = None
        with open(lua_scriptdir  + '/' + filename, 'r') as f:
            script = f.read()
        return self.execute(script, None, filename, sandboxed)

    def to_lua_val(self, val):
        rval = val
        if isinstance(val, QJSValue):
            rval = val.toVariant()
        if isinstance(val, list):
            rval = self.py_list_to_lua_table([self.to_lua_val(v) for v in val])
        elif isinstance(val, dict):
            rval = self.py_dict_to_lua_table({k:self.to_lua_val(v) for k,v in val.items()})
        return rval

    ######################
    # PROPERTIES
    ######################

    ###########
    ## SLOTS
    ###########

    @Slot('QVariant')
    def use_context(self, context_id):
        self.logger.debug(lambda: "Using context: {}".format(context_id))
        self.execute('__shoop_use_context({})'.format(context_id if context_id != None else 'nil'), None, None, True, False)
        self._current_context_id = context_id
    
    @Slot(result='QVariant')
    def current_context(self):
        return self._current_context_id
    
    @Slot(result='QVariant')
    def new_context(self):
        self.logger.debug(lambda: "Creating new context.")
        return self.evaluate('return __shoop_new_context()', None, None, True, False)
    
    @Slot('QVariant')
    def delete_context(self, context_id):
        self.logger.debug(lambda: "Deleting context: {}".format(context_id))
        if self.current_context() == context_id:
            self.use_context(None)
        self.execute('__shoop_delete_context({})'.format(context_id), None, None, True, False)
    
    @Slot('QVariant', result=bool)
    def context_exists(self, context_id):
        return bool(self.evaluate('return __shoop_context_exists({})'.format(context_id)))

    @Slot(str, 'QVariant', 'QVariant', bool, bool, result='QVariant')
    def evaluate(self, lua_code, context=None, script_name=None, sandboxed=True, catch_errors=True):
        try:
            self.logger.trace(lambda: 'evaluate (context {}):\n{}'.format(context, lua_code))
            prev_context = self.current_context()
            if context:
                if not self.context_exists(context):
                    raise ScriptExecutionError('Context {} does not exist.'.format(context))
                self.use_context(context)
            if not sandboxed:
                rval = self.lua.eval(lua_code)
            else:
                rval = self.run_sandboxed(lua_code)
            if context:
                self.use_context(prev_context)
            self.logger.trace(lambda: 'evaluate result: {}'.format(rval))
            if lupa.lua_type(rval) == 'function':
                # wrap the function in a lambda which will call it in the correct context
                rval = lambda *args, rval=rval, context=context: self.call(rval, args, context)
            return rval
        except Exception as e:
            if not catch_errors:
                raise e from e
            if script_name:
                self.logger.error(lambda: 'Error evaluating script "{}": {}. Trace: {}'.format(script_name, str(e), traceback.format_exc()))
            else:
                self.logger.error(lambda: 'Error evaluating expression: {}. Trace: {}'.format(str(e), traceback.format_exc()))
    
    @Slot(str, 'QVariant', 'QVariant', bool, bool)
    def execute(self, lua_code, context=None, script_name=None, sandboxed=True, catch_errors=True):
        try:
            self.logger.trace(lambda: 'Execute (context {}):\n{}'.format(context, lua_code))
            prev_context = self.current_context()
            if context:
                if not self.context_exists(context):
                    raise ScriptExecutionError('Context {} does not exist.'.format(context))
                self.use_context(context)
            if not sandboxed:
                self.lua.execute(lua_code)
            else:
                self.run_sandboxed(lua_code)
            if context:
                self.use_context(prev_context)
        except Exception as e:
            if not catch_errors:
                raise e from e
            if script_name:
                self.logger.error(lambda: 'Error executing script "{}": {}. Trace: {}'.format(script_name, str(e), traceback.format_exc()))
            else:
                self.logger.error(lambda: 'Error executing statement: {}. Trace: {}'.format(str(e), traceback.format_exc()))
    
    @Slot('QVariant', list, 'QVariant', result='QVariant')
    def call(self, callable, args=[], context=None):
        self.logger.trace(lambda: 'call callable in context {}:\n{}'.format(context, callable))
        if context:
            prev_context = self.current_context()
            self.use_context(context)
        rval = callable(*args)
        if context:
            self.use_context(prev_context)
        return rval
    
    @Slot(str, 'QVariant')
    def create_lua_qobject_interface_as_global(self, name, qobject):
        self.logger.debug(lambda: 'Creating Lua interface for QObject: {}'.format(qobject))
        module = create_lua_qobject_interface(self, qobject)
        self.global_registrar(name, module)
        

    ##########
    ## INTERNAL MEMBERS
    ##########
