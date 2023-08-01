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

# Add a -h option to the standard Qt options
for idx,arg in enumerate(sys.argv[1:]):
    if arg == '-h':
        sys.argv[idx+1] = '-help'

def to_c_chars(strings):
    numParams    = len(strings)
    strArrayType = c_char_p * numParams
    strArray     = strArrayType()
    for i, param in enumerate(strings):
        if isinstance(param, bytes):
            strArray[i] = c_char_p(param)
        else:
            strArray[i] = c_char_p(param.encode('utf-8'))
    return cast(strArray, POINTER(POINTER(c_char)))

reporter = JUnitTestReporter(parent=None)
setup = Setup(reporter, parent=None)

raw_setup = cast(Shiboken.getCppPointer(setup)[0], c_void_p)
exitcode = qml_tests.run_quick_test_main_with_setup(len(sys.argv), to_c_chars(sys.argv), 'shoopdaloop_tests', script_dir + '/..', raw_setup)

exit(exitcode)