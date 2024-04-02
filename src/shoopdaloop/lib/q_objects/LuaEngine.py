from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQml import QJSValue
from PySide6.QtQuick import QQuickItem

from .ShoopPyObject import *

import lupa
import copy
import traceback
import glob

from ..logging import Logger
from ..lua_qobject_interface import create_lua_qobject_interface

import os
import inspect
from ..directories import scripts_dir
lua_scriptdir = scripts_dir() + '/lib/lua'

class ScriptExecutionError(Exception):
    pass

class LuaEngine(ShoopQQuickItem):
    def __init__(self, parent=None):
        super(LuaEngine, self).__init__(parent)
        self.logger = Logger('Frontend.LuaEngine')
        self.lua_logger = Logger('Frontend.LuaScript')
        self.logger.debug(lambda: 'Initializing Lua runtime.')
        self.lua = lupa.LuaRuntime()
        self._G_registrar = self.lua.eval('function(name, fn) _G[name] = fn end')
        self._G_registrar('__shoop_print', lambda s, l=self.lua_logger: l.info('{}'.format(s)))
        self._G_registrar('__shoop_print_trace', lambda s, l=self.lua_logger: l.trace('{}'.format(s)))
        self._G_registrar('__shoop_print_debug', lambda s, l=self.lua_logger: l.debug('{}'.format(s)))
        self._G_registrar('__shoop_print_info', lambda s, l=self.lua_logger: l.info('{}'.format(s)))
        self._G_registrar('__shoop_print_warning', lambda s, l=self.lua_logger: l.warning('{}'.format(s)))
        self._G_registrar('__shoop_print_error', lambda s, l=self.lua_logger: l.error('{}'.format(s)))
        self.preload_libs()
        self.execute_builtin_script('system/sandbox.lua', False)
        self.run_sandboxed = self.lua.eval('function (code) return __shoop_run_sandboxed(code) end')
        self.execute('package.path = package.path .. ";{}"'.format(lua_scriptdir.replace('\\', '\\\\') + '/lib/?.lua'), None, True, False)
        self._G_registrar = lambda name, val: self.evaluate('return function(val) {} = val end'.format(name), None, True, False)(val)
        self._G_registrar('require', lambda s, self=self: self.require(s))
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
    
    def preload_libs(self):
        self.preloaded_libs = dict()
        libs = glob.glob(lua_scriptdir + '/lib/*.lua')
        for lib in libs:
            name = os.path.basename(lib).split('.')[0]
            self.logger.debug(lambda: 'Preloading Lua library: {}'.format(lib))
            content = None
            with open(lib, 'r') as f:
                content = f.read()
            self.preloaded_libs[name] = content

    def require(self, name):
        if name == 'shoop_control':
            # special case
            return self.evaluate('return __shoop_control_interface', None, True, False)
        if not name in self.preloaded_libs.keys():
            raise Exception('No such library in curated "require"-list: {}'.format(name))
        return self.evaluate(self.preloaded_libs[name], None, True, False)

    def execute_builtin_script(self, filename, sandboxed=True):
        self.logger.debug(lambda: 'Running built-in script: {}'.format(filename))
        script = None
        with open(lua_scriptdir  + '/' + filename, 'r') as f:
            script = f.read()
        return self.execute(script, filename, sandboxed)


    def to_lua_val(self, val):
        rval = val
        if isinstance(rval, QJSValue):
            if rval.isCallable():
                rval = lambda *args: val.call(*args)
            else:
                rval = rval.toVariant()
        if isinstance(rval, list):
            rval = self.py_list_to_lua_table([self.to_lua_val(v) for v in rval])
        elif isinstance(rval, dict):
            rval = self.py_dict_to_lua_table({k:self.to_lua_val(v) for k,v in rval.items()})
        return rval

    ######################
    # PROPERTIES
    ######################

    ###########
    ## SLOTS
    ###########
    @ShoopSlot(str, 'QVariant', bool, bool, result='QVariant')
    def evaluate(self, lua_code, script_name=None, sandboxed=True, catch_errors=True):
        try:
            self.logger.trace(lambda: 'evaluate:\n{}'.format(lua_code))
            if not sandboxed:
                rval = self.lua.eval(lua_code)
            else:
                rval = self.run_sandboxed(lua_code)
            self.logger.trace(lambda: 'evaluate result: {}'.format(rval))
            if lupa.lua_type(rval) == 'function':
                # wrap the function in a lambda which will call it
                rval = lambda *args, rval=rval: self.call(rval, args)
            return rval
        except Exception as e:
            if not catch_errors:
                raise e from e
            if script_name:
                self.logger.error(lambda: 'Error evaluating script "{}": {}. Trace: {}'.format(script_name, str(e), traceback.format_exc()))
            else:
                self.logger.error(lambda: 'Error evaluating expression: {}. Trace: {}'.format(str(e), traceback.format_exc()))
    
    @ShoopSlot(str, 'QVariant', bool, bool)
    def execute(self, lua_code, script_name=None, sandboxed=True, catch_errors=True):
        try:
            self.logger.trace(lambda: 'Execute:\n{}'.format(lua_code))
            if not sandboxed:
                self.lua.execute(lua_code)
            else:
                self.run_sandboxed(lua_code)
        except Exception as e:
            if not catch_errors:
                raise e from e
            if script_name:
                self.logger.error(lambda: 'Error executing script "{}": {}. Trace: {}'.format(script_name, str(e), traceback.format_exc()))
            else:
                self.logger.error(lambda: 'Error executing statement: {}. Trace: {}'.format(str(e), traceback.format_exc()))
    
    @ShoopSlot('QVariant', list, bool, result='QVariant')
    def call(self, callable, args=[], convert_args=False):
        if convert_args:
            args = [self.to_lua_val(a) for a in args]
        self.logger.trace(lambda: 'call callable:\n{}, args {}'.format(callable, args))
        rval = callable(*args)
        return rval
    
    @ShoopSlot(str, 'QVariant')
    def create_lua_qobject_interface_as_global(self, name, qobject):
        self.logger.debug(lambda: 'Creating Lua interface for QObject: {}'.format(qobject))
        module = create_lua_qobject_interface(self, qobject)
        self._G_registrar(name, module)
    
    @ShoopSlot()
    def stop(self):
        del self.lua
        self.logger.debug(lambda: 'Lua runtime closed.')

    ##########
    ## INTERNAL MEMBERS
    ##########
