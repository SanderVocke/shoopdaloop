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

import qml_tests
from ctypes import *

class Setup(QObject):
    def __init__(self, parent=None):
        super(Setup, self).__init__(parent)

    @Slot(QQmlEngine)
    def qmlEngineAvailable(self, engine):
        register_shoopdaloop_qml_classes()
        self.root_context_items = create_and_populate_root_context(engine, self)

argv = (POINTER(c_char) * len(sys.argv))()
for idx, arg in enumerate(sys.argv):
    argv[idx] = cast(arg.encode('ascii'), POINTER(c_char))
setup = Setup(None)
raw_setup = cast(Shiboken.getCppPointer(setup)[0], c_void_p)
qml_tests.run_quick_test_main_with_setup(len(argv), argv, 'shoopdaloop_tests', script_dir + '/..', raw_setup)