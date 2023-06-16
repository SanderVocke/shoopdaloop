import QtQuick 6.3
import QtTest 1.0
import Logger

import '../../backend/frontend_interface/types.js' as Types

TestCase {
    id: root
    property string name : 'UnnamedTestCase'
    property var logger : Logger { name: `Test.` + root.name }

    function verify_loop_cleared(loop) {
        verify(loop.mode == Types.LoopMode.Stopped, `Loop not stopped: ${loop.mode}. Trace: ${backtrace()}`)
        verify(loop.length == 0, `Loop length not 0: ${loop.length}. Trace: ${backtrace()}`)
    }

    function backtrace() {
        return new Error().stack
    }

    function verify_eq(a, b) {
        if (a != b) { logger.error(`Trace: ${backtrace()}`)}
        verify(a == b, `a != b (a = ${a}, b = ${b})`)
    }

    function verify_gt(a, b) {
        if (a <= b) { logger.error(`Trace: ${backtrace()}`)}
        verify(a > b, `a !> b (a = ${a}, b = ${b})`)
    }

    function start_test_fn(name) {
        logger.info("------------------------------------------------")
        logger.info(`Test start: ${name}`)
        logger.info("------------------------------------------------")
    }

    function end_test_fn(name) {
        logger.info(`Test end: ${name}`)
        logger.info("------------------------------------------------")
    }
}