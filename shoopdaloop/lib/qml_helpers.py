from PySide6.QtQml import qmlRegisterType

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
from lib.q_objects.ClickTrackGenerator import ClickTrackGenerator
from lib.q_objects.Task import Task
from lib.q_objects.Tasks import Tasks
from lib.q_objects.FXChain import FXChain
from lib.q_objects.FetchChannelData import FetchChannelData
from lib.q_objects.FloatsToImageString import FloatsToImageString
#from lib.q_objects.MIDIControlManager import MIDIControlManager
#from lib.q_objects.MIDIControlLink import MIDIControlLink
#from lib.q_objects.MIDIControlDialect import MIDIControlDialect
#from lib.q_objects.MIDIControlInputRule import MIDIControlInputRule

def register_qml_class(t, name):
    qmlRegisterType(t, name, 1, 0, name)

def register_shoopdaloop_qml_classes():
    register_qml_class(AudioPort, 'AudioPort')
    register_qml_class(MidiPort, 'MidiPort')
    register_qml_class(Loop, 'Loop')
    register_qml_class(FXChain, 'FXChain')
    register_qml_class(LoopAudioChannel, 'LoopAudioChannel')
    register_qml_class(LoopMidiChannel, 'LoopMidiChannel')
    register_qml_class(ClickTrackGenerator, 'ClickTrackGenerator')
    #register_qml_class(MIDIControlManager, 'MIDIControlManager')
    #register_qml_class(MIDIControlLink, 'MIDIControlLink')
    #register_qml_class(MIDIControlDialect, 'MIDIControlDialect')
    #register_qml_class(MIDIControlInputRule, 'BacMIDIControlInputRulekend')
    register_qml_class(Backend, 'Backend')
    register_qml_class(Task, 'Task')
    register_qml_class(Tasks, 'Tasks')
    register_qml_class(FetchChannelData, 'FetchChannelData')
    register_qml_class(FloatsToImageString, 'FloatsToImageString')

