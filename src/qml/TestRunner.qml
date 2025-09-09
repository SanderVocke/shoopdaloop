Item {
    id: root

    readonly property ShoopRustLogger logger : ShoopRustLogger { name: "Frontend.Qml.TestRunner" }
    property var running_testcase : null
    property var registered_testcases: []
    property var testcase_results: ({})
    property var ran_testcases: []
    property string filter_regex: ".*"

    readonly property bool done: {
        logger.debug(`Registered testcases: ${registered_testcases.length}. Ran: ${ran_testcases.length}`)
        return registered_testcases.length > 0 &&
               ran_testcases.length == registered_testcases.length;
    }
    onDoneChanged: {
        if (done) { testcase_done() }
    }
    
    signal testcase_done()

    function register_testcase(testcase) {
        logger.info(`Registering testcase: ${testcase.name}`)
        registered_testcases.push(testcase)
        registered_testcasesChanged(); //for recalc
    }

    function testcase_ready_to_start(testcase) {
        logger.info(`Testcase ${testcase} ready to start`)
        run_testcase(testcase)
    }

    function testcase_ran_callback(testcase_obj, fn_name, status) {
        var name = testcase_obj.name
        if (!(name in testcase_results)) {
            testcase_results[name] = {}
        }
        testcase_results[name][fn_name] = status
        testcase_resultsChanged()
    }

    function testcase_register_fn(testcase_obj, fn_name) {
        var name = testcase_obj.name
        if (!(name in testcase_results)) {
            testcase_results[name] = {}
        }
        testcase_results[name][fn_name] = "fail"
        testcase_resultsChanged()
    }

    function should_skip(function_name) {
        return !(new RegExp(filter_regex)).test(function_name)
    }

    function run_testcase(testcase_obj) {
        running_testcase = testcase_obj
        logger.info(`------------------------------------------------------`)
        logger.info(`-- testcase ${testcase_obj.name}`)
        logger.info(`------------------------------------------------------`)
        testcase_obj.run()
        running_testcase = null
        ran_testcases.push(testcase_obj)
        ran_testcasesChanged()
    }
}