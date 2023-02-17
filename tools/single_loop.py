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
from lib.backend import LoopMode, PortDirection
from lib.qml_helpers import register_qml_class
import time

print ("Starting a single loop with stereo audio and MIDI.")
app = QGuiApplication(sys.argv)
be = Backend('single_loop', app)
loop = be.create_loop()
audio1 = loop.add_audio_channel()
audio2 = loop.add_audio_channel()
midi = loop.add_midi_channel()
audio1_in = be.open_audio_port('audio_in_1', PortDirection.Input)
audio1_out = be.open_audio_port('audio_out_1', PortDirection.Output)
audio2_in = be.open_audio_port('audio_in_2', PortDirection.Input)
autio2_out = be.open_audio_port('audio_out_2', PortDirection.Output)
midi_in = be.open_midi_port('midi_in', PortDirection.Input)
midi_out = be.open_midi_port('midi_out', PortDirection.Output)
audio1.connect(audio1_in)
audio1.connect(audio1_out)
audio2.connect(audio1_in)
audio2.connect(audio1_out)
midi.connect(midi_in)
midi.connect(midi_out)

be.start_update_timer(30)

register_qml_class(Backend, "Backend")
register_qml_class(BackendLoop, "BackendLoop")

engine = QQmlApplicationEngine()
engine.quit.connect(app.quit)
engine.rootContext().setContextProperty("rootcontext_backend_loop", loop)
engine.load('{}/../lib/qml/single_loop_main.qml'.format(script_dir))

#loop.modeChanged.connect(lambda mode : print('Loop -> mode {}'.format(mode)))
exitcode = app.exec()