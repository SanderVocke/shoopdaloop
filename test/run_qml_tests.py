#!/bin/python

import os
import sys

from PyQt6.QtCore import QObject, pyqtSlot
from PyQt6.QtQml import QQmlEngine
from PyQt6.sip import unwrapinstance
script_dir = os.path.dirname(os.path.realpath(__file__))
sys.path.append(script_dir + '/..')
sys.path.append(script_dir + '/../build/backend/test/qt')

from lib.backend_interface import terminate as terminate_backend
from lib.qml_helpers import register_shoopdaloop_qml_classes
from lib.q_objects.SchemaValidator import SchemaValidator
from lib.q_objects.FileIO import FileIO
import qml_tests
from ctypes import *

class Setup(QObject):
    def __init__(self, parent=None):
        super(Setup, self).__init__(parent)

    @pyqtSlot(QQmlEngine)
    def qmlEngineAvailable(self, engine):
        register_shoopdaloop_qml_classes()
        self._validator = SchemaValidator()
        self._file_io = FileIO()
        engine.rootContext().setContextProperty("schema_validator", self._validator)
        engine.rootContext().setContextProperty("file_io", self._file_io)

argv = (POINTER(c_char) * len(sys.argv))()
for idx, arg in enumerate(sys.argv):
    argv[idx] = cast(arg.encode('ascii'), POINTER(c_char))
setup = Setup(None)
raw_setup = cast(unwrapinstance(setup), c_void_p)
qml_tests.run_quick_test_main_with_setup(len(argv), argv, 'shoopdaloop_tests', script_dir + '/..', raw_setup)