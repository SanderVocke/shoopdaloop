
from .q_objects.LuaEngine import LuaEngine
from .q_objects.ControlHandler import ControlHandler
from .q_objects.ShoopPyObject import *
from .lua_qobject_interface import create_lua_qobject_interface, lua_str

from PySide6.QtCore import QObject, Slot

import pytest

def test_init():
    eng = LuaEngine()
    create_lua_qobject_interface(eng, QObject())

def test_callback():
    class TestHandler(ShoopQObject):
        def __init__(self, parent=None):
            super(TestHandler, self).__init__(parent)
            pass

        lua_interfaces = [
            ['foo'],
            ['foz', lua_str]
        ]
        
        lua_constants = [
            ['const1', 'hello']
        ]

        @ShoopSlot(list, 'QVariant', result=str)
        def foo(self, args, engine):
            return 'bar'
        
        @ShoopSlot(list, 'QVariant', result=str)
        def foz(self, args, engine):
            return args[0]
    
    obj = TestHandler()
    eng = LuaEngine()
    eng.create_lua_qobject_interface_as_global('testy', obj)

    assert(eng.evaluate('return testy.foo()') == 'bar')
    assert(eng.evaluate('return testy.foz("baz")') == 'baz')
    assert(eng.evaluate('return testy.constants.const1') == 'hello')