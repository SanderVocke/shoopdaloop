#!/bin/python

import sys
import os
script_dir = os.path.dirname(os.path.realpath(__file__))
sys.path.append(script_dir + '/..')

from PyQt6.QtGui import QGuiApplication
from PyQt6.QtQml import QQmlApplicationEngine

from lib.q_objects.BackendAudioPort import BackendAudioPort
from lib.q_objects.BackendMidiPort import BackendMidiPort
from lib.q_objects.BackendLoop import BackendLoop
from lib.q_objects.BackendLoopAudioChannel import BackendLoopAudioChannel
from lib.q_objects.BackendLoopMidiChannel import BackendLoopMidiChannel
from lib.q_objects.Backend import Backend
from lib.q_objects.PortPair import PortPair
from lib.backend import LoopMode, PortDirection
from lib.qml_helpers import register_shoopdaloop_qml_classes
import time

print ("Starting a single loop with stereo audio and MIDI.")
app = QGuiApplication(sys.argv)
be = Backend('single_loop', app)
be.start_update_timer(30)

register_shoopdaloop_qml_classes()

engine = QQmlApplicationEngine()
engine.quit.connect(app.quit)
engine.rootContext().setContextProperty("backend", be)
engine.load('{}/../lib/qml/single_loop_main.qml'.format(script_dir))

exitcode = app.exec()