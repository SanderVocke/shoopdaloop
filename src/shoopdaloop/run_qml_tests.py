#!/bin/python

import os
import sys

from PySide6.QtCore import QObject, Slot
from PySide6.QtQml import QQmlEngine
from shiboken6 import Shiboken

script_dir = os.path.dirname(__file__)
sys.path.append(script_dir + '/..')

from shoopdaloop.libshoopdaloop_bindings import terminate as terminate_backend
from shoopdaloop.lib.qml_helpers import *
from shoopdaloop.lib.q_objects.SchemaValidator import SchemaValidator
from shoopdaloop.lib.logging import Logger
from shoopdaloop.lib.q_objects.JUnitTestReporter import JUnitTestReporter

import qml_tests
from ctypes import *

class Setup(QObject):
    def __init__(self, reporter, parent=None):
        super(Setup, self).__init__(parent)
        self.reporter = reporter

    @Slot(QQmlEngine)
    def qmlEngineAvailable(self, engine):
        register_shoopdaloop_qml_classes()
        self.root_context_items = create_and_populate_root_context(engine)
        engine.rootContext().setContextProperty('g_test_reporter', self.reporter)

logger = Logger('Test.Runner')

def usage():
    print('Usage: {} [--testfile | -f TESTFILE] [--junit report.xml] [--help | -h]'.format(sys.argv[0]))
    print('Options:')
    # print('  --testfile | -f TESTFILE: specify a file selector. Multiple may be used.')
    print('  --junit | -j report.xml:  use JUnit reporting and write to the given filename.')
    print('  --help | -h:              display this help.')

# Qt test frameworks has its own argument handling, but most of those are unneeded
# and it also doesn't provide a -h / --help. Parse our own and generate the correct
# args for Qt instead.
maybe_junit_filename = None
maybe_file_selectors = []
ignore_arg = False
for idx, arg in enumerate(sys.argv[1:]):
    maybe_next_arg = sys.argv[idx+2] if len(sys.argv) > (idx+2) else None
    if ignore_arg:
        ignore_arg = False
        continue
    elif (arg == '--junit' or arg == '-j') and maybe_next_arg:
        maybe_junit_filename = maybe_next_arg
        ignore_arg = True
    # elif (arg == '--testfile' or arg == '-f') and maybe_next_arg:
    #     maybe_file_selectors.append(maybe_next_arg)
    #     ignore_arg = True
    elif (arg == '--help' or arg == '-h'):
        usage()
        exit(0)
    else:
        print('Invalid argument: {}'.format(arg))
        usage()
        exit(1)

# Generate args to pass on
# argv = (POINTER(c_char) * (len(maybe_file_selectors)*2))()
# for idx, s in enumerate(maybe_file_selectors):
#     print(s)
#     argv[idx*2] = cast('-file-selector'.encode('ascii'), POINTER(c_char))
#     argv[idx*2+1] = cast(s.encode('ascii'), POINTER(c_char))
argv = (POINTER(c_char) * 0)()

reporter = JUnitTestReporter(parent=None)
setup = Setup(reporter, parent=None)

raw_setup = cast(Shiboken.getCppPointer(setup)[0], c_void_p)
qml_tests.run_quick_test_main_with_setup(len(argv), argv, 'shoopdaloop_tests', script_dir + '/..', raw_setup)

if maybe_junit_filename:
    logger.info('Generating JUnit output into: {}'.format(maybe_junit_filename))
    junit = reporter.generate_junit()
    with open(maybe_junit_filename, 'w') as f:
        f.write(junit)
    logger.info('JUnit:\n{}'.format(junit))