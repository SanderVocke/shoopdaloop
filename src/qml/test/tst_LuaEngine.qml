import QtQuick 6.6

import '..'
import ShoopConstants
import ShoopDaLoop.Rust
import './testfilename.js' as TestFilename

ShoopTestFile {
    LuaEngine {
        id: engine
    }

    ShoopTestCase {
        name: 'LuaEngine'
        filename : TestFilename.test_filename()

        test_fns: ({
            'test_evaluate_sandboxed': () => {
                verify_eq(engine.evaluate('return 1 + 2', null, true), 3)
                verify_eq(engine.evaluate('return "hello world"', null, true), "hello world")
                verify_eq(engine.evaluate('return {a = 1}', null, true), { a: 1 })
            }
        })
    }
}