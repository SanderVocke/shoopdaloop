from PySide6.QtQml import qmlRegisterType, qmlRegisterSingletonType, qmlTypeId, QQmlComponent
from PySide6.QtCore import QUrl
from PySide6.QtQuick import QQuickItem

import weakref

import sys
import os
import glob
import re
import importlib

from .directories import *

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
from .q_objects.RenderAudioWaveform import RenderAudioWaveform
from .q_objects.RenderMidiSequence import RenderMidiSequence
from .q_objects.TestCase import TestCase
from .q_objects.OSUtils import OSUtils
from .q_objects.DummyProcessHelper import DummyProcessHelper
from .q_objects.CompositeLoop import CompositeLoop

from .logging import Logger as BareLogger
from .js_constants import create_js_constants

import time

# import PySide6.QtCore as QtCore

# qt_logger = BareLogger("Frontend.Qt")
# qt_message_handler_installed = False

# def qt_msg_handler(mode, context, message):
#     pass
#     # fn = None
#     # if mode == QtCore.QtInfoMsg:
#     #     fn = qt_logger.info
#     # elif mode == QtCore.QtWarningMsg:
#     #     fn = qt_logger.warning
#     # elif mode == QtCore.QtCriticalMsg:
#     #     fn = qt_logger.error
#     # elif mode == QtCore.QtFatalMsg:
#     #     fn = qt_logger.error
#     # else:
#     #     fn = qt_logger.debug
    
#     #fn("%s: %s (%s:%d, %s)" % (mode, message, context.file, context.line, context.file))
            
def install_qt_message_handler():
    global qt_message_handler_installed
    global qt_logger
    if not qt_message_handler_installed:
        QtCore.qInstallMessageHandler(qt_msg_handler)
        qt_message_handler_installed = True

# Read version from the version.txt file (will be present when packaged)
pkg_version = None
with open(installation_dir() + '/version.txt', 'r') as f:
    pkg_version = f.read().strip()

def register_qml_class(t, name):
    qmlRegisterType(t, "ShoopDaLoop.Python" + name, 1, 0, "Python" + name)

def create_constants_instance(engine):
    return create_js_constants(engine)

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
    register_qml_class(RenderAudioWaveform, 'RenderAudioWaveform')
    register_qml_class(RenderMidiSequence, 'RenderMidiSequence')
    register_qml_class(TestCase, 'TestCase')
    register_qml_class(DummyProcessHelper, 'DummyProcessHelper')
    register_qml_class(CompositeLoop, 'CompositeLoop')

    qmlRegisterSingletonType("ShoopConstants", 1, 0, "ShoopConstants", create_constants_instance)
    
    # install_qt_message_handler()

def create_and_populate_root_context(engine, global_args, additional_items={}):
    def create_component(path):
        comp = QQmlComponent(engine, QUrl.fromLocalFile(path))
        while comp.status() == QQmlComponent.Loading:
            time.sleep(0.05)
        if comp.status() != QQmlComponent.Ready:
            raise Exception('Failed to load {}: {}'.format(path, str(comp.errorString())))
        return comp
    
    # Constants definition for Javascript side
    constants = create_js_constants(engine)
    engine.registerModule("shoop_js_constants", constants)
    
    # QML instantiations
    registries_comp = create_component(scripts_dir() + '/lib/qml/AppRegistries.qml')
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
        'os_utils': OSUtils(parent=engine)
    }

    for key, item in additional_items.items():
        items[key] = item

    items['default_logger'].name = 'Frontend.Qml'

    items['app_metadata'].version_string = pkg_version

    for key, item in items.items():
        engine.rootContext().setContextProperty(key, item)
    
    return items

