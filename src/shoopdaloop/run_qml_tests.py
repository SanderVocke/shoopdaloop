#!/usr/bin/python

import os
import sys

# Before importing the back-end library, we need to make sure that it will be able
# to find the Qt libraries it needs. We do this by searching for the Qt libraries in the
# PySide package.
from shoopdaloop.lib.ensure_ld_qt import ensure_ld_qt
ensure_ld_qt()

from PySide6.QtCore import QObject, Slot
from PySide6.QtQml import QQmlEngine
from shiboken6 import Shiboken

script_dir = os.path.dirname(__file__)
sys.path.append(script_dir + '/..')

from shoopdaloop.libshoopdaloop_bindings import terminate_backend
from shoopdaloop.lib.qml_helpers import *
from shoopdaloop.lib.q_objects.SchemaValidator import SchemaValidator
from shoopdaloop.lib.logging import Logger
from shoopdaloop.lib.backend_wrappers import BackendType
from shoopdaloop.lib.q_objects.QoverageCollectorFactory import QoverageCollectorFactory

from ctypes import *

import qml_tests

qoverage_collector_factory = QoverageCollectorFactory()

class Setup(QObject):
    def __init__(self, parent=None):
        super(Setup, self).__init__(parent)

    @Slot(QQmlEngine)
    def qmlEngineAvailable(self, engine):
        register_shoopdaloop_qml_classes()
        self.root_context_items = create_and_populate_root_context(
            engine,
            { 'backend_type': BackendType.Dummy.value, 'backend_argstring': '' },
            { 'qoverage_collector_factory' : qoverage_collector_factory }
            )

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

setup = Setup(parent=None)

raw_setup = cast(Shiboken.getCppPointer(setup)[0], c_void_p)
exitcode = qml_tests.run_quick_test_main_with_setup(len(sys.argv), to_c_chars(sys.argv), 'shoopdaloop_tests', script_dir + '/..', raw_setup)

qoverage_collector_factory.report_all()

final_msg = 'Tests ran successfully' if exitcode == 0 else 'Test(s) failed or failed to run'
print('{}. Exiting with code {}'.format(final_msg, exitcode))

exit(exitcode)