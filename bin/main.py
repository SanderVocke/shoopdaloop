#!/bin/python

import sys
import os
import traceback
script_dir = os.path.dirname(os.path.realpath(__file__))
sys.path.append(script_dir + '/..')

from PyQt6.QtGui import QGuiApplication
from PyQt6.QtQml import QQmlApplicationEngine
from lib.qml_helpers import register_shoopdaloop_qml_classes
from lib.q_objects.SchemaValidator import SchemaValidator
from lib.q_objects.FileIO import FileIO

validator = SchemaValidator()
file_io = FileIO()

app = QGuiApplication(sys.argv)
register_shoopdaloop_qml_classes()
engine = QQmlApplicationEngine()
engine.quit.connect(app.quit)
engine.rootContext().setContextProperty("schema_validator", validator)
engine.rootContext().setContextProperty("file_io", file_io)
engine.load('{}/../lib/qml/applications/shoopdaloop_main.qml'.format(script_dir))

exitcode = app.exec()