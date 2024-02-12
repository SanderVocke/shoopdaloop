from .LuaEngine import LuaEngine
from .ControlHandler import ControlHandler
from ..lua_qobject_interface import create_lua_qobject_interface
import pytest

def test_control_handler_loop_selector_coords():
    obj = ControlHandler()
    eng = LuaEngine()
    eng.create_lua_qobject_interface_as_global('shoop', obj)

    eng.evaluate('shoop.loop_get_mode({4,5})')
    
    assert(obj.cached_calls() == [
        ['loop_get_mode', [4,5]]
    ])

def test_control_handler_loop_selector_coordslist():
    obj = ControlHandler()
    eng = LuaEngine()
    eng.create_lua_qobject_interface_as_global('shoop', obj)

    eng.evaluate('shoop.loop_get_mode({ {5,6}, {7,8} })')
    
    assert(obj.cached_calls() == [
        ['loop_get_mode', [[5,6], [7,8]] ]
    ])

def test_control_handler_loop_selector_simple_functor():
    obj = ControlHandler()
    eng = LuaEngine()
    eng.create_lua_qobject_interface_as_global('shoop', obj)

    eng.evaluate('shoop.loop_get_mode(function(loop) return true end)')
    
    assert(len(obj.cached_calls()) == 1)
    assert(obj.cached_calls()[0][0] == 'loop_get_mode')
    assert(callable(obj.cached_calls()[0][1]))
    assert(obj.cached_calls()[0][1](None) == True)

    obj.clear_call_cache()

    eng.evaluate('shoop.loop_get_mode(function(loop) return false end)')
    
    assert(len(obj.cached_calls()) == 1)
    assert(obj.cached_calls()[0][0] == 'loop_get_mode')
    assert(callable(obj.cached_calls()[0][1]))
    assert(obj.cached_calls()[0][1](None) == False)

def test_control_handler_loop_selector_object_functor():
    obj = ControlHandler()
    eng = LuaEngine()
    eng.create_lua_qobject_interface_as_global('shoop', obj)

    eng.evaluate('shoop.loop_get_mode(function(loop) return loop["x"] < 10 end)')
    
    assert(len(obj.cached_calls()) == 1)
    assert(obj.cached_calls()[0][0] == 'loop_get_mode')
    fn = obj.cached_calls()[0][1]
    assert (fn({ "x": 9 }) == True)
    assert (fn({ "x": 10 }) == False)