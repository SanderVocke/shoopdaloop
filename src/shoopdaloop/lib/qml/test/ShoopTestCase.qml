import QtQuick 6.3
import QtTest 1.0
import ShoopDaLoop.PythonLogger

import '../../generated/types.js' as Types
import './testDeepEqual.js' as TestDeepEqual

TestCase {
    id: root
    property string name : 'UnnamedTestCase'
    property string filename : 'UnknownTestFile'
    property var logger : PythonLogger { name: `Test.` + root.name }

    Component.onCompleted: logger.info("Testcase " + name + " created.")
    Component.onDestruction: logger.info("Testcase " + name + " destroyed.")

    function verify_loop_cleared(loop) {
        verify_eq(loop.mode, Types.LoopMode.Stopped)
        verify_eq(loop.length, 0)
    }

    function cleanup() {
        logger.debug("cleanup " + root.name)
        // Note: the fact that this works is due to the internal TestCase.qml implementation of Qt Quick Test.
        // Tested on Qt 6.3
        var failed = qtest_results.failed || qtest_results.dataFailed
        var skipped = qtest_results.skipped
        var statusdesc = failed ? 'fail' :
                         skipped ? 'skip' :
                         'pass'
        var casename = name
        var func = qtest_results.functionName
        logger.info(`${filename}::${casename}::${func}: ${statusdesc}`)
    }

    function format_error(failstring, stack=null) {
        let _stack = stack ? stack : (new Error().stack)
        let m = _stack.match(/[\s]*(test_[^\@]+)\@.*\/(tst_.*\.qml:[0-9]*)/)
        if (m) {
            let testfn = m[1]
            let testfile = m[2]
            let trace = '  ' + _stack.split('\n').join('\n  ')
            return `${testfn}: ${failstring} \@ ${testfile}\nBacktrace:\n${trace}`
        }
        let trace = '  ' + _stack.split('\n').join('\n  ')
        return `${failstring}\nBacktrace:\n${trace}`
        return failstring        
    }

    function verify_throw(fn) {
        try {
            fn()
        } catch (e) {
            verify(true)
            return;
        }
        logger.error(format_error(`verify_throw failed (fn = ${fn})`))
        verify(false)
    }

    function verify_true(a) {
        var result = Boolean(a)
        let failstring = `verify_true failed (v = ${a})`
        if (!result) {
            logger.error(format_error(failstring))
        }
        verify(result, failstring)
    }

    function verify_eq(a, b, stringify=false) {
        var result;

        let failstring = ''
        if (stringify) {
            failstring = `verify_eq failed (a = ${JSON.stringify(a, null, 2)}, b = ${JSON.stringify(b, null, 2)})`
        } else {
            failstring = `verify_eq failed (a = ${a}, b = ${b})`
        }

        let compare = (a, b) => {
            if (Array.isArray(a) && Array.isArray(b)) {
                return TestDeepEqual.testArraysCompare(a, b, compare);
            } else if (TestDeepEqual.isObject(a) && TestDeepEqual.isObject(b)) {
                return a == b || TestDeepEqual.testDeepEqual(a, b);
            } else {
                return a == b;
            }
        }
        result = compare(a,b);
        
        if (!result) {
            logger.error(format_error(failstring))
        }
        verify(result, failstring)
    }

    function verify_approx(a, b) {
        var result;
        let compare = (a,b) => a == b || ((a - b) < Math.max(a,b) / 10000.0)
        let failstring = `verify_approx failed (a = ${a}, b = ${b})`
        if (Array.isArray(a) && Array.isArray(b)) {
            result = TestDeepEqual.testArraysCompare(a, b, compare);
        } else {
            result = compare(a, b)
        }
        if (!result) {
            logger.error(format_error(failstring))
        }
        verify(result, failstring)
    }

    function verify_gt(a, b) {
        let failstring = `verify_gt failed (a = ${a}, b = ${b})`
        let result = a > b;
        if (!result) {
            logger.error(format_error(failstring))
        }
        verify(result, failstring)
    }

    function start_test_fn(name) {
        logger.info("------------------------------------------------")
        logger.info(`START ${name}`)
        logger.info("------------------------------------------------")
    }

    function end_test_fn(name) {
        logger.info("------------------------------------------------")
        logger.info(`END ${name}`)
        logger.info("------------------------------------------------")
    }

    function run_case(name, fn) {
        try {
            start_test_fn(name)
            fn()
            end_test_fn(name)
        } catch (e) {
            let failstring = `Uncaught exception: ${e.message} (${e.name}}`
            logger.error(format_error(failstring, e.stack))
            throw e
        }
    }
}