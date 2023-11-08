from PySide6.QtQml import qmlRegisterType, qmlTypeId, QQmlComponent
from PySide6.QtCore import QUrl
from PySide6.QtQuick import QQuickItem

import weakref

import sys
import os
import glob
import re
import importlib

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
from .q_objects.LuaEngine import LuaEngine
from .q_objects.DictTreeModel import DictTreeModelFactory
from .q_objects.ReleaseFocusNotifier import ReleaseFocusNotifier
from .q_objects.ControlInterface import ControlInterface
from .q_objects.MidiControlPort import MidiControlPort
from .q_objects.SettingsIO import SettingsIO
from .q_objects.TestScreenGrabber import TestScreenGrabber

from .logging import Logger as BareLogger

import time

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
    register_qml_class(LuaEngine, 'LuaEngine')
    register_qml_class(DictTreeModelFactory, 'DictTreeModelFactory')
    register_qml_class(ControlHandler, 'ControlHandler')
    register_qml_class(ReleaseFocusNotifier, 'ReleaseFocusNotifier')
    register_qml_class(ControlInterface, 'ControlInterface')
    register_qml_class(MidiControlPort, 'MidiControlPort')
    register_qml_class(SettingsIO, 'SettingsIO')
    register_qml_class(TestScreenGrabber, 'TestScreenGrabber')

def create_and_populate_root_context(engine, global_args, additional_items={}):
    def create_component(path):
        comp = QQmlComponent(engine, QUrl.fromLocalFile(path))
        while comp.status() == QQmlComponent.Loading:
            time.sleep(0.05)
        if comp.status() != QQmlComponent.Ready:
            raise Exception('Failed to load {}: {}'.format(path, str(comp.errorString())))
        return comp

    # Set import path to predefined classes
    engine.addImportPath(script_dir + '/../qml_types')
    engine.addPluginPath(script_dir + '/../qml_plugins')

    # Check if our C++ extensions are available. If not, provide fallbacks.
    for path in glob.glob(script_dir + '/qml/extension_checks/check_*.qml'):
        filename = os.path.basename(path)
        match = re.match(r'^check_(.+)\.qml$', filename)
        if not match:
            raise Exception('Invalid extension check file: ' + path)
        extension_name = match.group(1)
        l = BareLogger('Frontend.ExtensionCheck')
        try:
            #create_component(path)
            type_id = qmlTypeId(extension_name, 1, 0, extension_name)
            if type_id < 0:
                raise Exception('boo')
        except Exception as e:
            l.warning(lambda: 'QML extension {} not available. Using fallback. Exception: {}'.format(extension_name, str(e)))
            # module_name = 'shoopdaloop.lib.q_objects.extension_fallbacks.{}'.format(extension_name)
            # module = importlib.import_module(module_name)
            # cl = getattr(module, extension_name)
            # qmlRegisterType(cl, extension_name, 1, 0, extension_name)
    
    # QML instantiations
    registries_comp = create_component(script_dir + '/qml/AppRegistries.qml')
    registries = registries_comp.create()

    items = {
        'file_io': FileIO(parent=engine),
        'schema_validator': SchemaValidator(parent=engine),
        'click_track_generator': ClickTrackGenerator(parent=engine),
        'key_modifiers': KeyModifiers(parent=engine),
        'app_metadata': ApplicationMetadata(parent=engine),
        'default_logger': Logger(),
        'tree_model_factory': DictTreeModelFactory(parent=engine),
        'release_focus_notifier': ReleaseFocusNotifier(parent=engine),
        'global_args': global_args,
        'settings_io': SettingsIO(parent=engine),
        'registries': registries,
        'screen_grabber': TestScreenGrabber(weak_engine=weakref.ref(engine), parent=engine),
    }

    for key, item in additional_items.items():
        items[key] = item

    items['default_logger'].name = 'Frontend.Qml'

    items['app_metadata'].version_string = pkg_version

    for key, item in items.items():
        engine.rootContext().setContextProperty(key, item)
    
    return items

