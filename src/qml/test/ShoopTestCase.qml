import QtQuick 6.6

import ShoopDaLoop.Rust

import './testDeepEqual.js' as TestDeepEqual
import '../js/type_checks.js' as TypeChecks
import '..'

Item {
    id: root
    property string filename : 'UnknownTestFile'
    property var logger : ShoopRustLogger { name: `Frontend.Qml.ShoopTestCase` }

    property bool print_error_traces: ShoopRustOSUtils.get_env_var("QMLTEST_NO_ERROR_TRACES") == null

    // It seems the built-in test function filter of the QML test runner is not working.
    // Provide a means to only run a subset of tests.
    property var testfn_filter: null

    property bool when: true
    property bool _internal_triggered: false
    property bool registered: false

    property string name: 'UnnamedTestCase'
    property bool skip: false

    onWhenChanged: update_next_cycle.trigger()

    function update_trigger() {
        if (when && !_internal_triggered) {
            _internal_triggered = true
            logger.debug("Ready to start")
            shoop_test_file_runner.testcase_runner.testcase_ready_to_start(root)
        } else {
            logger.debug(`Not ready to start yet: ${when}, ${_internal_triggered}`)
        }
    }

    function get_all_test_methods() {
        var metaObject = root.metaObject()
        var methods = []

        // Iterate over the methods and print their names
        for (var i = 0; i < metaObject.methodCount(); i++) {
            var method = metaObject.method(i)
            methods.push(method.name())
        }
    }

    property var test_fns: ({})
    property var testcase_init_fn: () => { root.wait(50) }
    property var testcase_deinit_fn: () => {}

    ExecuteNextCycle {
        id: update_next_cycle
        onExecute: { root.update_trigger() }
    }

    function maybe_register() {
        if(shoop_test_file_runner.testcase_runner &&
           shoop_test_file_runner.testcase_runner.objectName == "TestRunner" &&
           !root.registered) {
                logger.info(`Testcase ${name} created (${Object.keys(test_fns).length} tests).`);

                if (testfn_filter) {
                    logger.warning("Unimplemented: filtering")
                    logger.warning(`Testcase ${name} has a test function filter: ${testfn_filter}`)
                }

                shoop_test_file_runner.testcase_runner.register_testcase(root)
                update_next_cycle.trigger()
                root.registered = true
           }
    }

    Component.onCompleted: {
        maybe_register()
    }
    Connections {
        target: shoop_test_file_runner
        function onTestcase_runnerChanged() { maybe_register() }
    }
    Component.onDestruction: logger.info(`Testcase ${name} destroyed.`)

    function section(title) {
        logger.debug(`Test section: ${title}`)
    }

    function verify_loop_cleared(loop) {
        verify_eq(loop.mode, ShoopRustConstants.LoopMode.Stopped)
        verify_eq(loop.length, 0)
    }

    function format_error(failstring, stack=null) {
        let _stack = stack ? stack : (new Error().stack)
        let m = _stack.match(/[\s]*(test_[^\@]+)\@.*\/(tst_.*\.qml:[0-9]*)/)
        if (m) {
            let testfn = m[1]
            let testfile = m[2]
            let trace = '  ' + _stack.split('\n').join('\n  ')
            if (print_error_traces) {
                return `${testfn}: ${failstring} \@ ${testfile}\nBacktrace:\n${trace}`
            } else {
                return `${testfn}: ${failstring} \@ ${testfile}`
            }
        }
        let trace = '  ' + _stack.split('\n').join('\n  ')

        if (print_error_traces) {
            return `${failstring}\nBacktrace:\n${trace}`
        } else {
            return failstring
        }
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

    function verify_true(a, msg=`verify_true failed`) {
        var result = Boolean(a)
        let failstring = `${msg} (v = ${a})`
        if (!result) {
            logger.error(format_error(failstring))
        }
        verify(result, failstring)
    }

    function verify_eq(a, b, msg=null, stringify=null) {
        var result;

        if (stringify === null) {
            stringify = (typeof a === 'object' && typeof b === 'object')
        }

        let failstring = ''
        if (stringify) {
            failstring = `verify_eq failed (a = ${JSON.stringify(a, null, 2)}, b = ${JSON.stringify(b, null, 2)})`
        } else {
            failstring = `verify_eq failed (a = ${a}, b = ${b})`
        }
        if(msg) {
            failstring = failstring + " - " + msg
        }

        function compare(a, b) {
            if (TypeChecks.is_array(a) && TypeChecks.is_array(b)) {
                return TestDeepEqual.testArraysCompare(a, b, compare);
            } else if (TypeChecks.is_object(a) && TypeChecks.is_object(b)) {
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

    function verify_approx(a, b, margin=0.001, do_print=true) {
        var result;
        function compare (a,b) { return (a == b || (Math.abs(a - b) < Math.max(Math.abs(a),Math.abs(b)) * margin)) }
        let failstring = `verify_approx failed`
        if (do_print) {
            failstring += ` (a = ${a}, b = ${b})`
        }
        if (TypeChecks.is_array(a) && TypeChecks.is_array(b)) {
            result = TestDeepEqual.testArraysCompare(a, b, compare, do_print ? console.log : (msg) => {});
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
        ShoopRustCrashHandling.set_json_tag("shoop_action", `qml test (${name})`)
        logger.info(`===== TEST START ${name}`)
    }

    function end_test_fn(name) {
        ShoopRustCrashHandling.set_json_tag("shoop_action", "qml test (none)")
        logger.info(`===== TEST END ${name}`)
    }

    function run_case(name, fn) {
        try {
            if (testfn_filter && testfn_filter !== name) {
                skip("Test function filter active, no match")
                return
            }
            start_test_fn(name)
            fn()
            end_test_fn(name)
        } catch (e) {
            let failstring = `Uncaught exception: ${e.message} (${e.name})`
            logger.error(format_error(failstring, e.stack))
            throw e
        }
    }

    function wait(ms) {
        shoop_application.wait(ms)
    }

    function wait_condition(condition, timeout=2000, msg=`condition not met in time`) {
        var waited = 5
        wait(waited)
        while(!condition() && waited <= timeout) {
            wait(5)
            waited += 5
        }
        verify_true(condition(), msg)
    }

    function wait_session_loaded(session) {
        wait_condition(() => session.loaded, 10000, `session not loaded in time`)
    }

    function wait_session_io_done() {
        root.logger.debug("Start wait session IO done")
        wait_condition(() => !AppRegistries.state_registry.io_active, 2000, "Session I/O not finished in time")
        root.logger.debug("Session IO done.")
    }

    function connectOnce(sig, slot) {
        var f = function() {
            slot.apply(this, arguments)
            sig.disconnect(f)
        }
        sig.connect(f)
    }

    function wait_updated(backend) {
        function wait_once() {
            var done = false
            function updated() {
                done = true
            }
            connectOnce(backend.updated_on_gui_thread, updated)
            wait_condition(() => done == true, 500, "Backend not updated in time")
        }
        wait_once()
        wait_once()
        wait_once()
    }

    function wait_controlled_mode(backend) {
        wait_updated(backend)
        while(backend.last_processed != 0) {
            wait_updated(backend)
        }
    }

    property var current_testcase: {
        'name': 'none',
        'checks': 0,
        'failures': 0,
        'skips': 0,
        'passes': 0,
        'time': 0.0
    }

    function verify(condition, msg) {
        current_testcase.checks += 1
        if (!condition) {
            current_testcase.failures += 1
        } else {
            current_testcase.passes += 1
        }
    }

    function should_skip(fn) {
        let full_name = name + "::" + fn
        return shoop_test_file_runner.testcase_runner.should_skip(full_name)
    }

    function skip(msg) {
        if (current_testcase) {
            let fullname = root.name + "::" + current_testcase.name
            logger.info(`skipping ${fullname}`)
            current_testcase.skips += 1
        }
    }

    function fail(msg) {
        verify_true(false, msg)
    }

    function run() {
        logger.info(`Running testcase ${name}`)

        logger.debug("running testcase_init_fn")
        testcase_init_fn()

        for (var key in test_fns) {
            shoop_test_file_runner.testcase_runner.testcase_register_fn(root, key)
        }

        for (var key in test_fns) {
            let fullname = root.name + "::" + key
            var status = 'skip'
            try {
                current_testcase = {
                    'name': key,
                    'checks': 0,
                    'failures': 0,
                    'skips': 0,
                    'passes': 0,
                    'time': 0.0
                }
                if (!should_skip(key)) {
                    logger.info(`~~~~ running ${fullname} ~~~~`)
                    var start = new Date()
                    test_fns[key]()
                    var end = new Date()
                    current_testcase.time = (end - start) * 0.001
                    logger.debug(`Passed ${current_testcase.passes} of ${current_testcase.checks} checks in ${current_testcase.name}`)
                } else {
                    logger.info(`skipping ${fullname}`)
                }
                if(
                    current_testcase.passes > 0 &&
                    current_testcase.failures == 0 &&
                    current_testcase.skips == 0
                ) {
                    status = 'pass'
                } else if(current_testcase.failures > 0) {
                    status = 'fail'
                }
            } catch(error) {
                status = 'fail'
                logger.error(format_error(error.message, error.stack))
            }
            shoop_test_file_runner.testcase_runner.testcase_ran_callback(root, key, status)
            current_testcase = null
        }

        logger.debug("running testcase_deinit_fn")
        testcase_deinit_fn()
        var casename = name
        if (status == 'fail') {
            logger.error(`----------     ${filename}::${casename}: FAIL`)
        } else {
            logger.info(`----------     ${filename}::${casename}: ${status.toUpperCase()}`)
        }
    }
}