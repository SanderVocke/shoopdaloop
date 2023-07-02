from .ScriptingEngine import ScriptingEngine
import lupa
import pytest

def test_init():
    eng = ScriptingEngine()
    assert(eng != None)

    assert(eng.eval('1 + 1') == 2)

def test_callback():
    eng = ScriptingEngine()
    eng.define_global_callback(lambda: "HELLO WORLD", 'test_python_callback')

    assert(eng.eval('test_python_callback()') == "HELLO WORLD")

def test_sandbox():
    eng = ScriptingEngine()
    unsafe_command = 'load("print(\"hello world\")")'
    safe_command = 'print("hello world")'

    eng.eval(safe_command)
    eng.execute(safe_command)

    with pytest.raises(lupa.LuaError):
        eng.eval(unsafe_command)
    with pytest.raises(lupa.LuaError):
        eng.execute(unsafe_command)

def test_nonexistent_global():
    eng = ScriptingEngine()
    with pytest.raises(lupa.LuaError):
        eng.eval('nonexisting')
    
def test_declare_global():
    eng = ScriptingEngine()
    eng.execute('declare_global("existing", "Hello World!")')
    assert(eng.eval('existing') == 'Hello World!')
    eng.execute('declare_global("existing", "Should not overwrite")')
    assert(eng.eval('existing') == 'Hello World!')

def test_declare_new_global():
    eng = ScriptingEngine()
    eng.execute('declare_new_global("existing", "Hello World!")')
    assert(eng.eval('existing') == 'Hello World!')
    with pytest.raises(lupa.LuaError):
        eng.execute('declare_new_global("existing", "Should throw")')
    assert(eng.eval('existing') == 'Hello World!')

def test_context_declare_none_active():
    eng = ScriptingEngine()
    with pytest.raises(lupa.LuaError):
        eng.eval('declare_in_context("foo")')
    with pytest.raises(lupa.LuaError):
        eng.eval('declare_new_in_context("foo")')

def test_declare_in_context():
    eng = ScriptingEngine()
    c = eng.new_context()
    # declare
    eng.execute('declare_in_context("foo", "bar")', c)

    # invalid (declare without context)
    with pytest.raises(lupa.LuaError):
        eng.execute('declare_in_context("foa", "faz")')

    # get
    assert(eng.eval('foo', c) == 'bar')

    # invalid get (no context passed)
    with pytest.raises(lupa.LuaError):
        eng.eval('foo')
    
    # redeclare (ignored)
    eng.execute('declare_in_context("foo", "baf")', c)
    assert(eng.eval('foo', c) == 'bar')

    # modify
    eng.execute('foo = "baz"', c)
    assert(eng.eval('foo', c) == 'baz')
    # still no global leak after modification
    with pytest.raises(lupa.LuaError):
        assert(eng.eval('foo'))

def test_declare_new_in_context():
    eng = ScriptingEngine()
    c = eng.new_context()
    # declare
    eng.execute('declare_new_in_context("foo", "bar")', c)

    # invalid (redeclare)
    with pytest.raises(lupa.LuaError):
        eng.execute('declare_new_in_context("foo", "bar")')

def test_two_contexts():
    eng = ScriptingEngine()
    c1 = eng.new_context()
    c2 = eng.new_context()

    # declare
    eng.execute('declare_new_in_context("foo", "bar1")', c1)
    eng.execute('declare_new_in_context("foo", "bar2")', c2)

    # get
    assert(eng.eval('foo', c1) == 'bar1')
    assert(eng.eval('foo', c2) == 'bar2')
    with pytest.raises(lupa.LuaError):
        eng.eval('foo')
