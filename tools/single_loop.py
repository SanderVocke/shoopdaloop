#!/bin/python

import sys
import os
import traceback
script_dir = os.path.dirname(os.path.realpath(__file__))
sys.path.append(script_dir + '/..')

from PyQt6.QtGui import QGuiApplication
from PyQt6.QtQml import QQmlApplicationEngine
from lib.qml_helpers import register_shoopdaloop_qml_classes

print ("Starting a single loop with stereo audio and MIDI.")
app = QGuiApplication(sys.argv)
register_shoopdaloop_qml_classes()
engine = QQmlApplicationEngine()
engine.quit.connect(app.quit)
engine.load('{}/../lib/qml/single_loop_main.qml'.format(script_dir))

exitcode = app.exec()