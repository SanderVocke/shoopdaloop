#!/usr/bin/python
exit_text_printed = False
coverage_reported = False
exit_text = ''

def exit_handler(reason=None):
    global exit_text_printed
    global coverage_reported
    if not exit_text_printed:
        if reason:
            print("Failing because process abnormally terminated ({}). Test results overview:".format(reason))
        print(exit_text)
        exit_text_printed = True

def signal_handler(signum, frame):
    exit_handler('signal {}'.format(signum))

def run_qml_tests(args):
    global exit_text
    
    import os
    import glob
    import atexit
    import signal
    import time
    import sys

    from PySide6.QtCore import QObject, Slot
    from PySide6.QtTest import QTest
    from shiboken6 import Shiboken
    import json
    from xml.dom import minidom

    from shoopdaloop.libshoopdaloop_bindings import set_global_logging_level, log_level_error
    from shoopdaloop.lib.qml_helpers import register_qml_class
    from shoopdaloop.lib.q_objects.SchemaValidator import SchemaValidator
    from shoopdaloop.lib.logging import Logger
    from shoopdaloop.lib.backend_wrappers import AudioDriverType
    from shoopdaloop.lib.q_objects.QoverageCollectorFactory import QoverageCollectorFactory
    from shoopdaloop.lib.q_objects.Application import Application
    from shoopdaloop.lib.q_objects.TestRunner import TestRunner
    from shoopdaloop.lib.directories import installation_dir, scripts_dir

    import argparse
    import re

    parser = argparse.ArgumentParser()
    parser.add_argument('test_file_glob_pattern', nargs=argparse.REMAINDER, help='Glob pattern(s) for test QML files.')
    parser.add_argument('--list', '-l', action='store_true', help="Don't run but list all found test functions.")
    parser.add_argument('--filter', '-f', default=None, help='Regex filter for testcases.')
    parser.add_argument('--junit-xml', '-j', default=None, help='ShoopPortDirection_Output file for JUnit XML report.')
    parser.add_argument('-d', '--qml-debug', metavar='PORT', type=int, help='Start QML debugging on PORT')
    parser.add_argument('-w', '--debug-wait', action='store_true', help='With QML debugging enabled, will wait until debugger connects.')
    args = parser.parse_args(args=args)

    qoverage_collector_factory = QoverageCollectorFactory()

    logger = Logger('QmlTests')

    logger.info('Creating test application...')

    global_args = {
        'backend_type': AudioDriverType.Dummy.value,
        'load_session_on_startup': None,
        'test_grab_screens': None,
        'monkey_tester': False
    }

    test_files = glob.glob(scripts_dir() + '/**/tst_*.qml', recursive=True)
    if len(args.test_file_glob_pattern):
        test_files = []
        for pattern in args.test_file_glob_pattern:
            test_files += glob.glob(pattern, recursive=True)

    runner = TestRunner()
    if args.list:
        runner.should_skip = lambda fn: True
    elif args.filter:
        if args.filter[-1] != '$':
            args.filter += '$'
        runner.should_skip = lambda fn: not bool(re.match(args.filter, fn))

    additional_root_context = {
        'qoverage_collector_factory' : qoverage_collector_factory,
        'shoop_test_runner': runner
    }

    app = Application(
        'ShoopDaLoop QML Tests',
        None,
        global_args,
        additional_root_context,
        args.qml_debug,
        args.debug_wait
    )

    app.exit_handler_called.connect(lambda: sys.exit(1))

    for file in test_files:
        filename = os.path.basename(file)    
        print()
        logger.info('===== Test file: {} ====='.format(filename))
        
        app.reload_qml(file, False)    
        
        while not runner.done:
            app.processEvents()
            time.sleep(0.001)
        
        app.unload_qml()
        app.wait(50)

    # count totals
    passed = 0
    failed = 0
    skipped = 0

    results = runner.results
    failed_cases = []
    skipped_cases = []
    for case, case_results in results.items():
        for fn, fn_result in case_results.items():
            if args.list:
                print(case + '::' + fn)
            if fn_result == 'pass':
                passed += 1
            elif fn_result == 'skip':
                skipped += 1
                skipped_cases.append(case + '::' + fn)
            elif fn_result == 'fail':
                failed += 1
                failed_cases.append(case + '::' + fn)

            else:
                raise Exception('Unknown result: {}'.format(fn_result))

    # Make sure we gather the results before exiting the app, so that if shutdown issues occur,
    # the user still gets to see the test overview.

    final_result = (
        1 if failed > 0 else 0
    )
    final_result_readable = (
        'FAIL' if final_result else ('PASS WITH SKIPS' if skipped > 0 else 'PASS')
    )
    maybe_failures_string = ''
    if failed > 0:
        maybe_failures_string = '''

    Failed cases: {}
    '''.format(json.dumps(failed_cases, indent=2))
    maybe_skip_string = ''
    if skipped > 0:
        maybe_skip_string = '''

    Skipped cases: {}
    '''.format(json.dumps(skipped_cases, indent=2))    

    # TODO: this is nasty, but it's a quick hack to get the test results to show up last
    set_global_logging_level(log_level_error)

    exit_text = '''
    ========================================================
    Test run finished. Overall result: {}.
    ========================================================

    Totals:
    - Testcases: {}
    - Passed: {}
    - Failed: {}
    - Skipped: {}{}{}
    '''.format(final_result_readable, passed+failed+skipped, passed, failed, skipped, maybe_failures_string, maybe_skip_string)

    atexit.register(exit_handler)
    signal.signal(signal.SIGTERM, signal_handler)
    signal.signal(signal.SIGINT, signal_handler)
    signal.signal(signal.SIGABRT, signal_handler)

    if args.junit_xml:
        root = minidom.Document()
        testsuites = root.createElement('testsuites')
        root.appendChild(testsuites)

        for case, case_results in results.items():
            testsuite = root.createElement('testsuite')
            testsuite.setAttribute('name', case)
            testsuites.appendChild(testsuite)

            for fn, fn_result in case_results.items():
                testcase = root.createElement('testcase')
                testcase.setAttribute('name', fn)
                testcase.setAttribute('classname', case)
                testsuite.appendChild(testcase)

                if fn_result == 'pass':
                    pass
                elif fn_result == 'skip':
                    skipped_elem = root.createElement('skipped')
                    testcase.appendChild(skipped_elem)
                elif fn_result == 'fail':
                    failure_elem = root.createElement('failure')
                    testcase.appendChild(failure_elem)
                else:
                    raise Exception('Unknown result: {}'.format(fn_result))
        
        print("Writing JUnit XML to {}".format(args.junit_xml))
        root.writexml(open(args.junit_xml, 'w'), indent='  ', addindent='  ', newl='\n')

    if not coverage_reported:
        qoverage_collector_factory.report_all()
        app.wait(1000)

    app.do_quit()

    if args.list:
        return 0

    return final_result