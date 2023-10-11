
from .q_objects.ScriptingEngine import ScriptingEngine
from .q_objects.ControlHandler import ControlHandler
from .lua_qobject_interface import create_lua_qobject_interface, lua_str

from PySide6.QtCore import QObject, Slot

import pytest

def test_init():
    eng = ScriptingEngine()
    create_lua_qobject_interface('testinterface', eng, QObject())

def test_callback():
    class TestHandler(QObject):
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

        @Slot(result=str)
        def foo(self):
            return 'bar'
        
        @Slot(str, result=str)
        def foz(self, arg):
            return arg
    
    obj = TestHandler()
    eng = ScriptingEngine()
    create_lua_qobject_interface('testy', eng, obj)

    assert(eng.evaluate('return testy.foo()') == 'bar')
    assert(eng.evaluate('return testy.foz("baz")') == 'baz')
    assert(eng.evaluate('return testy.constants.const1') == 'hello')