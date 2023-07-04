
from .q_objects.ScriptingEngine import ScriptingEngine
from .q_objects.ControlHandler import ControlHandler
from .lua_qobject_interface import create_lua_qobject_interface, lua_passthrough
import pytest

def test_init():
    eng = ScriptingEngine()
    create_lua_qobject_interface('testinterface', eng, None)

def test_callback():
    class TestHandler:
        def __init__(self):
            pass

        lua_interfaces = [
            ['foo'],
            ['foz', lua_passthrough]
        ]

        def foo(self):
            return 'bar'
        
        def foz(self, arg):
            return arg
    
    obj = TestHandler()
    eng = ScriptingEngine()
    create_lua_qobject_interface('testy', eng, obj)

    assert(eng.eval('return testy.foo()') == 'bar')
    assert(eng.eval('return testy.foz("baz")') == 'baz')