Item {
    id: root

    readonly property PythonLogger logger : PythonLogger { name: "Frontend.Qml.TestRunner" }
    property var running_testcase : null
    property var registered_testcases: []
    property var testcase_results: ({})
    property var ran_testcases: []
    property string filter_regex: ".*"
    
    readonly property bool done: {
        return registered_testcases.length > 0 &&
               ran_testcases.length == registered_testcases.length;
    }

    function register_testcase(testcase) {
        logger.info(`Registering testcase: ${testcase}`)
        registered_testcases.push(testcase)
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
    }

    function testcase_register_fn(testcase_obj, fn_name) {
        var name = testcase_obj.name
        if (!(name in testcase_results)) {
            testcase_results[name] = {}
        }
        testcase_results[name][fn_name] = "fail"
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
    }
}