import QtQuick 6.3
import QtTest 1.0

import '../../generated/types.js' as Types
import './testfilename.js' as TestFilename
import '..'

ShoopTestCase {
    name: 'Lua'
    filename : TestFilename.test_filename()
    when: script1.ready && script2.ready

    LuaScript {
        id: script1
        script_name: "script1"
        script_code: "declare_in_context('my_var', 1)\n" +
                     "declare_global('my_global', 5)"
        catch_errors: false
    }
    LuaScript {
        id: script2
        script_name: "script2"
        script_code: "declare_in_context('my_var', 2)"
        catch_errors: false
    }
    LuaScript {
        id: script3
        script_name: "script3"
        script_code: "declare_in_context('other_var', 4)"
        catch_errors: false
    }

    function test_scripts() {
        run_case('test_scripts', () => {
            verify_eq(script1.evaluate('return my_var'), 1)
            verify_eq(script1.evaluate('return my_global'), 5)
            verify_eq(script2.evaluate('return my_var'), 2)
            verify_eq(script2.evaluate('return my_global'), 5)
            verify_throw(() => { return script1.evaluate('return other_var') })
            verify_throw(() => { return script2.evaluate('return other_var') })
            verify_eq(script3.evaluate('return other_var'), 4)
            verify_throw(() => { return script3.evaluate('return my_var') })
            verify_eq(script3.evaluate('return my_global'), 5)

            let fn = script1.evaluate('return function() return my_var end')
            scripting_engine.use_context(script2.scripting_context)
            verify_eq(scripting_engine.call(fn, [], null), 1)

            let context = script1.scripting_context
            script1.destroy()
            wait(20)
            verify_throw(() => { return scripting_engine.evaluate(
                'return my_var',
                context,
                'test',
                true,
                false
            ) })
            verify_throw(() => { return scripting_engine.evaluate(
                'return my_global',
                context,
                'test',
                true,
                false
            ) })
            verify_throw(() => { scripting_engine.call(fn, [], null) })
        })
    }
}