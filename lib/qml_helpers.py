from PyQt6.QtQml import qmlRegisterType

import sys
import os
script_dir = os.path.dirname(os.path.realpath(__file__))
sys.path.append(script_dir + '/..')

from lib.q_objects.BackendAudioPort import BackendAudioPort
from lib.q_objects.BackendMidiPort import BackendMidiPort
from lib.q_objects.BackendLoop import BackendLoop
from lib.q_objects.BackendLoopAudioChannel import BackendLoopAudioChannel
from lib.q_objects.BackendLoopMidiChannel import BackendLoopMidiChannel
from lib.q_objects.PortPair import PortPair
from lib.q_objects.Backend import Backend

def register_qml_class(t, name):
    qmlRegisterType(t, name, 1, 0, name)

def register_shoopdaloop_qml_classes():
    register_qml_class(BackendAudioPort, 'BackendAudioPort')
    register_qml_class(BackendMidiPort, 'BackendMidiPort')
    register_qml_class(BackendLoop, 'BackendLoop')
    register_qml_class(BackendLoopAudioChannel, 'BackendLoopAudioChannel')
    register_qml_class(BackendLoopMidiChannel, 'BackendLoopMidiChannel')
    register_qml_class(PortPair, 'PortPair')
    register_qml_class(Backend, 'Backend')

