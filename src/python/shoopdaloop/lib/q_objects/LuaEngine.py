from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQml import QJSValue
from PySide6.QtQuick import QQuickItem

from .ShoopPyObject import *

from .LuaEngineImpl import LuaEngineImpl
class LuaEngine(ShoopQQuickItem):
    def __init__(self, parent=None):
        super(LuaEngine, self).__init__(parent)
        self.impl = LuaEngineImpl()

    ###########
    ## SLOTS
    ###########
    @ShoopSlot(str, 'QString', bool, bool, result='QVariant')
    def evaluate(self, lua_code, script_name=None, sandboxed=True, catch_errors=True):
        self.impl.evaluate(str(lua_code), script_name, sandboxed, catch_errors)
    
    @ShoopSlot(str, 'QString', bool, bool)
    def execute(self, lua_code, script_name=None, sandboxed=True, catch_errors=True):
        self.impl.execute(str(lua_code), script_name, sandboxed, catch_errors)
    
    @ShoopSlot('QVariant', list, bool, result='QVariant')
    def call(self, callable, args=[], convert_args=False):
        self.impl.call(callable, args, convert_args)
    
    @ShoopSlot(str, 'QObject*')
    def create_lua_qobject_interface_as_global(self, name, qobject):
        self.impl.create_lua_qobject_interface_as_global(name, qobject)
    
    @ShoopSlot()
    def stop(self):
        self.impl.stop()
