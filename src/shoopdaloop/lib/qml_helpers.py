from PySide6.QtQml import qmlRegisterType

import sys
import os
script_dir = os.path.dirname(__file__)
sys.path.append(script_dir + '/..')

from .q_objects.AudioPort import AudioPort
from .q_objects.MidiPort import MidiPort
from .q_objects.Loop import Loop
from .q_objects.LoopAudioChannel import LoopAudioChannel
from .q_objects.LoopMidiChannel import LoopMidiChannel
from .q_objects.Backend import Backend
from .q_objects.ClickTrackGenerator import ClickTrackGenerator
from .q_objects.Task import Task
from .q_objects.Tasks import Tasks
from .q_objects.FXChain import FXChain
from .q_objects.FetchChannelData import FetchChannelData
from .q_objects.FileIO import FileIO
from .q_objects.SchemaValidator import SchemaValidator
from .q_objects.KeyModifiers import KeyModifiers
from .q_objects.ApplicationMetadata import ApplicationMetadata
from .q_objects.Logger import Logger
from .q_objects.ControlHandler import ControlHandler
from .q_objects.ScriptingEngine import ScriptingEngine
from .q_objects.DictTreeModel import DictTreeModelFactory
from .q_objects.ReleaseFocusNotifier import ReleaseFocusNotifier
from .q_objects.ControlInterface import ControlInterface
from .q_objects.MidiControlPort import MidiControlPort
from .q_objects.SettingsIO import SettingsIO
from .q_objects.AppRegistries import AppRegistries

# Read version from the version.txt file (will be present when packaged)
pkg_version = None
with open(script_dir + '/../version.txt', 'r') as f:
    pkg_version = f.read().strip()

def register_qml_class(t, name):
    qmlRegisterType(t, "ShoopDaLoop.Python" + name, 1, 0, "Python" + name)

def register_shoopdaloop_qml_classes():
    # Register Python classes
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
    register_qml_class(FileIO, 'FileIO')
    register_qml_class(SchemaValidator, 'SchemaValidator')
    register_qml_class(KeyModifiers, 'KeyModifiers')
    register_qml_class(ApplicationMetadata, 'ApplicationMetadata')
    register_qml_class(Logger, 'Logger')
    register_qml_class(ScriptingEngine, 'ScriptingEngine')
    register_qml_class(DictTreeModelFactory, 'DictTreeModelFactory')
    register_qml_class(ControlHandler, 'ControlHandler')
    register_qml_class(ReleaseFocusNotifier, 'ReleaseFocusNotifier')
    register_qml_class(ControlInterface, 'ControlInterface')
    register_qml_class(MidiControlPort, 'MidiControlPort')
    register_qml_class(SettingsIO, 'SettingsIO')
    register_qml_class(AppRegistries, 'AppRegistries')

def create_and_populate_root_context(engine, global_args, additional_items={}):
    # Set import path to predefined classes
    engine.addImportPath(script_dir + '/../qml_types')
    engine.addPluginPath(script_dir + '/../qml_plugins')

    items = {
        'file_io': FileIO(parent=engine),
        'schema_validator': SchemaValidator(parent=engine),
        'click_track_generator': ClickTrackGenerator(parent=engine),
        'key_modifiers': KeyModifiers(parent=engine),
        'app_metadata': ApplicationMetadata(parent=engine),
        'default_logger': Logger(),
        'tree_model_factory': DictTreeModelFactory(parent=engine),
        'scripting_engine': ScriptingEngine(parent=engine),
        'release_focus_notifier': ReleaseFocusNotifier(parent=engine),
        'global_args': global_args,
        'settings_io': SettingsIO(parent=engine),
        'registries': AppRegistries(parent=engine),
    }

    for key, item in additional_items.items():
        items[key] = item

    items['default_logger'].name = 'Frontend.Qml'

    items['app_metadata'].version_string = pkg_version

    for key, item in items.items():
        engine.rootContext().setContextProperty(key, item)
    
    return items

