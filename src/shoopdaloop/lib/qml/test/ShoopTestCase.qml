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

    function verify_loop_cleared(loop) {
        verify(loop.mode == Types.LoopMode.Stopped, `Loop not stopped: ${loop.mode}. Trace: ${backtrace()}`)
        verify(loop.length == 0, `Loop length not 0: ${loop.length}. Trace: ${backtrace()}`)
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

        if (g_test_reporter) {
            g_test_reporter.report_result (filename, casename, func, statusdesc)
        }
    }

    function backtrace() {
        return new Error().stack
    }

    function verify_eq(a, b) {
        var result;
        if (Array.isArray(a) && Array.isArray(b)) {
            result = TestDeepEqual.testArraysEqual(a, b);
        } else {
            result = a == b;
        }
        if (!result) { logger.error(`Trace: ${backtrace()}`)}
        verify(result, `a != b (a = ${a}, b = ${b})`)
    }

    function verify_gt(a, b) {
        if (a <= b) { logger.error(`Trace: ${backtrace()}`)}
        verify(a > b, `a !> b (a = ${a}, b = ${b})`)
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
}