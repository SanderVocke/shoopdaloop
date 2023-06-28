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
from lib.q_objects.FileIO import FileIO
from lib.q_objects.SchemaValidator import SchemaValidator
from lib.q_objects.KeyModifiers import KeyModifiers
from lib.q_objects.ApplicationMetadata import ApplicationMetadata
from lib.q_objects.Logger import Logger
from lib.q_objects.ControlHandler import ControlHandler
from lib.q_objects.ScriptingEngine import ScriptingEngine
from lib.q_objects.DictTreeModel import DictTreeModelFactory
from lib.cpp_imports import RenderAudioWaveform

script_dir = os.path.dirname(os.path.realpath(__file__))
version_file = script_dir + '/../../version.txt'

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
    register_qml_class(Backend, 'Backend')
    register_qml_class(Task, 'Task')
    register_qml_class(Tasks, 'Tasks')
    register_qml_class(FetchChannelData, 'FetchChannelData')
    register_qml_class(RenderAudioWaveform, 'RenderAudioWaveform')
    register_qml_class(FileIO, 'FileIO')
    register_qml_class(SchemaValidator, 'SchemaValidator')
    register_qml_class(KeyModifiers, 'KeyModifiers')
    register_qml_class(ApplicationMetadata, 'ApplicationMetadata')
    register_qml_class(Logger, 'Logger')
    register_qml_class(ScriptingEngine, 'ScriptingEngine')
    register_qml_class(DictTreeModelFactory, 'DictTreeModelFactory')
    register_qml_class(ControlHandler, 'ControlHandler')

def create_and_populate_root_context(engine, parent):
    items = {
        'file_io': FileIO(parent=parent),
        'schema_validator': SchemaValidator(parent=parent),
        'click_track_generator': ClickTrackGenerator(parent=parent),
        'key_modifiers': KeyModifiers(parent=parent),
        'app_metadata': ApplicationMetadata(parent=parent),
        'default_logger': Logger(parent=parent),
        'tree_model_factory': DictTreeModelFactory(parent=parent),
        'scripting_engine': ScriptingEngine(parent=parent)
    }

    items['default_logger'].name = 'Frontend.Qml'

    with open(version_file, 'r') as vf:
        items['app_metadata'].version_string = vf.read()

    for key, item in items.items():
        engine.rootContext().setContextProperty(key, item)
    
    return items

