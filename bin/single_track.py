#!/bin/python

import sys
import os
import traceback
script_dir = os.path.dirname(os.path.realpath(__file__))
sys.path.append(script_dir + '/..')

from PySide6.QtGui import QGuiApplication
from PySide6.QtQml import QQmlApplicationEngine
from lib.qml_helpers import register_shoopdaloop_qml_classes

print ("Starting a single track.")
app = QGuiApplication(sys.argv)
register_shoopdaloop_qml_classes()
engine = QQmlApplicationEngine()
engine.quit.connect(app.quit)
engine.load('{}/../lib/qml/applications/single_track_main.qml'.format(script_dir))

exitcode = app.exec()