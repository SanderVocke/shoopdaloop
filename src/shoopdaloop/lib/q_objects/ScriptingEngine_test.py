from .ScriptingEngine import ScriptingEngine
import lupa
import pytest

def test_init():
    eng = ScriptingEngine()
    assert(eng != None)

    assert(eng.evaluate('return 1 + 1') == 2)

def test_sandbox_security():
    eng = ScriptingEngine()
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

def test_sandbox_persistence():
    eng = ScriptingEngine()
    eng.execute('declare_global("a", "foo")')
    assert(eng.evaluate('return a') == "foo")

def test_nonexistent_global():
    eng = ScriptingEngine()
    with pytest.raises(lupa.LuaError):
        eng.evaluate('return nonexisting', catch_errors=False)
    
def test_declare_global():
    eng = ScriptingEngine()
    eng.execute('declare_global("existing", "Hello World!")')
    assert(eng.evaluate('return existing') == 'Hello World!')
    eng.execute('declare_global("existing", "Should not overwrite")')
    assert(eng.evaluate('return existing') == 'Hello World!')

def test_declare_new_global():
    eng = ScriptingEngine()
    eng.execute('declare_new_global("existing", "Hello World!")')
    assert(eng.evaluate('return existing') == 'Hello World!')
    with pytest.raises(lupa.LuaError):
        eng.execute('declare_new_global("existing", "Should throw")', catch_errors=False)
    assert(eng.evaluate('return existing') == 'Hello World!')

def test_context_declare_none_active():
    eng = ScriptingEngine()
    with pytest.raises(lupa.LuaError):
        eng.execute('declare_in_context("foo")', catch_errors=False)
    with pytest.raises(lupa.LuaError):
        eng.execute('declare_new_in_context("foo")', catch_errors=False)

def test_declare_in_context():
    eng = ScriptingEngine()
    c = eng.new_context()
    # declare
    eng.execute('declare_in_context("foo", "bar")', c)

    # invalid (declare without context)
    with pytest.raises(lupa.LuaError):
        eng.execute('declare_in_context("foa", "faz")', catch_errors=False)

    # get
    assert(eng.evaluate('return foo', c) == 'bar')

    # invalid get (no context passed)
    with pytest.raises(lupa.LuaError):
        eng.evaluate('return foo', catch_errors=False)
    
    # redeclare (ignored)
    eng.execute('declare_in_context("foo", "baf")', c)
    assert(eng.evaluate('return foo', c) == 'bar')

    # modify
    eng.execute('foo = "baz"', c)
    assert(eng.evaluate('return foo', c) == 'baz')
    # still no global leak after modification
    with pytest.raises(lupa.LuaError):
        assert(eng.evaluate('return foo', catch_errors=False))

def test_declare_new_in_context():
    eng = ScriptingEngine()
    c = eng.new_context()
    # declare
    eng.execute('declare_new_in_context("foo", "bar")', c)

    # invalid (redeclare)
    with pytest.raises(lupa.LuaError):
        eng.execute('declare_new_in_context("foo", "bar")', catch_errors=False)

def test_two_contexts():
    eng = ScriptingEngine()
    c1 = eng.new_context()
    c2 = eng.new_context()

    # declare
    eng.execute('declare_new_in_context("foo", "bar1")', c1)
    eng.execute('declare_new_in_context("foo", "bar2")', c2)

    # get
    assert(eng.evaluate('return foo', c1) == 'bar1')
    assert(eng.evaluate('return foo', c2) == 'bar2')
    with pytest.raises(lupa.LuaError):
        eng.evaluate('return foo', catch_errors=False)
