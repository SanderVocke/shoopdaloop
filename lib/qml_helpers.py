from PyQt6.QtQml import qmlRegisterType

import sys
import os
script_dir = os.path.dirname(os.path.realpath(__file__))
sys.path.append(script_dir + '/..')

from lib.q_objects.AudioPort import AudioPort
from lib.q_objects.MidiPort import MidiPort
from lib.q_objects.Loop import Loop
from lib.q_objects.LoopAudioChannel import LoopAudioChannel
from lib.q_objects.LoopMidiChannel import LoopMidiChannel
from lib.q_objects.Backend import Backend

def register_qml_class(t, name):
    qmlRegisterType(t, name, 1, 0, name)

def register_shoopdaloop_qml_classes():
    register_qml_class(AudioPort, 'AudioPort')
    register_qml_class(MidiPort, 'MidiPort')
    register_qml_class(Loop, 'Loop')
    register_qml_class(LoopAudioChannel, 'LoopAudioChannel')
    register_qml_class(LoopMidiChannel, 'LoopMidiChannel')
    register_qml_class(Backend, 'Backend')

