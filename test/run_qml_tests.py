#!/bin/python

import os
import sys

from PySide6.QtCore import QObject, Slot
from PySide6.QtQml import QQmlEngine
from shiboken6 import Shiboken

script_dir = os.path.dirname(os.path.realpath(__file__))
sys.path.append(script_dir + '/../shoopdaloop')
sys.path.append(script_dir + '/../build/backend/test/qt')

from lib.backend_interface import terminate as terminate_backend
from lib.qml_helpers import *
from lib.q_objects.SchemaValidator import SchemaValidator
from lib.logging import Logger
from lib.q_objects.JUnitTestReporter import JUnitTestReporter

import qml_tests
from ctypes import *

logger = Logger('Test.Runner')

class Setup(QObject):
    def __init__(self, reporter, parent=None):
        super(Setup, self).__init__(parent)
        self.reporter = reporter

    @Slot(QQmlEngine)
    def qmlEngineAvailable(self, engine):
        register_shoopdaloop_qml_classes()
        self.root_context_items = create_and_populate_root_context(engine, self)
        engine.rootContext().setContextProperty('g_test_reporter', self.reporter)

# Take out --junit file.xml args if any, rest are passed to Qt test framework
filtered_argv = sys.argv
maybe_junit_filename = None
for idx, arg in enumerate(sys.argv):
    if arg == '--junit' and len(sys.argv) > (idx+1):
        maybe_junit_filename = sys.argv[idx+1]
        del filtered_argv[idx+1]
        del filtered_argv[idx]

argv = (POINTER(c_char) * len(filtered_argv))()
for idx, arg in enumerate(filtered_argv):
    argv[idx] = cast(arg.encode('ascii'), POINTER(c_char))

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