from .LuaEngine import LuaEngine
import lupa
import pytest

def test_init():
    eng = LuaEngine()
    assert(eng != None)

    assert(eng.evaluate('return 1 + 1') == 2)

def test_sandbox_security():
    eng = LuaEngine()
    unsafe_command = 'load("print(\"hello world\")")'
    safe_command = 'print("hello world")'

    eng.evaluate(safe_command)
    eng.execute(safe_command)

    with pytest.raises(lupa.LuaError):
        eng.evaluate(unsafe_command, catch_errors=False)
    with pytest.raises(lupa.LuaError):
        eng.execute(unsafe_command, catch_errors=False)
    with pytest.raises(lupa.LuaError):
        eng.evaluate('_G.{}'.format(unsafe_command), catch_errors=False)
    with pytest.raises(lupa.LuaError):
        eng.evaluate('rawget(_G, "load")(print(\"hello world\"))', catch_errors=False)
    