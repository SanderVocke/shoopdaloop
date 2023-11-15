#!/usr/bin/python

import os
import sys
import glob

from PySide6.QtCore import QObject, Slot
from PySide6.QtQml import QQmlEngine
from PySide6.QtTest import QTest
from shiboken6 import Shiboken
import json

script_dir = os.path.dirname(__file__)
sys.path.append(script_dir + '/..')

from shoopdaloop.libshoopdaloop_bindings import terminate_backend
from shoopdaloop.lib.qml_helpers import *
from shoopdaloop.lib.q_objects.SchemaValidator import SchemaValidator
from shoopdaloop.lib.logging import Logger
from shoopdaloop.lib.backend_wrappers import BackendType
from shoopdaloop.lib.q_objects.QoverageCollectorFactory import QoverageCollectorFactory
from shoopdaloop.lib.q_objects.Application import Application
from shoopdaloop.lib.q_objects.TestRunner import TestRunner

import argparse
import re

parser = argparse.ArgumentParser()
parser.add_argument('test_file_glob_pattern', nargs=argparse.REMAINDER, help='Glob pattern(s) for test QML files.')
parser.add_argument('--list', '-l', action='store_true', help="Don't run but list all found test functions.")
parser.add_argument('--filter', '-f', default=None, help='Regex filter for testcases.')
args = parser.parse_args()

qoverage_collector_factory = QoverageCollectorFactory()

logger = Logger('run_qml_tests.py')

logger.info('Creating test application...')

global_args = {
    'backend_type': BackendType.Dummy.value,
    'backend_argstring': '',
    'load_session_on_startup': None,
    'test_grab_screens': None,
}

test_files = glob.glob(script_dir + '/**/tst_*.qml', recursive=True)
if len(args.test_file_glob_pattern):
    test_files = []
    for pattern in args.test_file_glob_pattern:
        test_files += glob.glob(pattern, recursive=True)

runner = TestRunner()
if args.list:
    runner.should_skip = lambda fn: True
elif args.filter:
    runner.should_skip = lambda fn: not bool(re.match(args.filter, fn))

additional_root_context = {
    'qoverage_collector_factory' : qoverage_collector_factory,
    'shoop_test_runner': runner
}

app = Application(
    'ShoopDaLoop QML Tests',
    None,
    global_args,
    additional_root_context
)

def exit_now():
    app.quit()
    exit(1)

app.exit_handler_called.connect(lambda: exit(1))

for file in test_files:
    filename = os.path.basename(file)
    print()
    logger.info('===== Test file: {} ====='.format(filename))
    
    app.reload(file, False)    
    
    while not runner.done:
        app.processEvents()
        time.sleep(0.001)

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

app.quit()

if args.list:
    exit(0)

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

print('''
========================================================
Test run finished. Overall result: {}.
========================================================

Totals:
- Testcases: {}
- Passed: {}
- Failed: {}
- Skipped: {}{}{}
'''.format(final_result_readable, passed+failed+skipped, passed, failed, skipped, maybe_failures_string, maybe_skip_string))

exit(final_result)