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
        if (stack == null) {
            stack = new Error().stack
        }
        let m = stack.match(/[\s]*(test_[^\@]+)\@.*\/(tst_.*\.qml:[0-9]*)/)
        let testfn = m[1]
        let testfile = m[2]
        let trace = '  ' + stack.split('\n').join('\n  ')
        return `${testfn}: ${failstring} \@ ${testfile}\nBacktrace:\n${trace}`
    }

    function verify_throw(a) {
        var result = Boolean(a)
        let failstring = `verify_throw failed (v = ${a})`
        if (!result) {
            logger.error(format_error(failstring))
        }
        verify(result, failstring)
    }

    function verify_eq(a, b) {
        var result;
        let failstring = `verify_eq failed (a = ${a}, b = ${b})`
        if (Array.isArray(a) && Array.isArray(b)) {
            result = TestDeepEqual.testArraysEqual(a, b);
        } else {
            result = a == b;
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

    function run_test_fn(name, fn) {
        try {
            start_test_fn(name)
            fn()
            end_test_fn(name)
        } catch (e) {
            let failstring = `Uncaught exception: ${e.message} (${e.name}}`
            logger.error(format_error(failstring, e.stack))
        }
    }
}