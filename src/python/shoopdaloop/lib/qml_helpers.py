from PySide6.QtQml import qmlRegisterType, qmlRegisterSingletonType, qmlTypeId, QQmlComponent
from PySide6.QtCore import QUrl
from PySide6.QtQuick import QQuickItem

import weakref

import sys
import os
import glob
import re
import importlib
import ctypes
import platform

from shoop_config import shoop_version, shoop_qml_dir
pkg_version = shoop_version

from .q_objects.LoopAudioChannel import LoopAudioChannel
from .q_objects.LoopMidiChannel import LoopMidiChannel
from .q_objects.ClickTrackGenerator import ClickTrackGenerator
from .q_objects.KeyModifiers import KeyModifiers
from .q_objects.ApplicationMetadata import ApplicationMetadata
from .q_objects.Logger import Logger
from .q_objects.ControlHandler import ControlHandler
from .q_objects.LuaEngine import LuaEngine
from .q_objects.DictTreeModel import DictTreeModelFactory
from .q_objects.ControlInterface import ControlInterface
from .q_objects.MidiControlPort import MidiControlPort
from .q_objects.SettingsIO import SettingsIO
from .q_objects.TestScreenGrabber import TestScreenGrabber
from .q_objects.RenderMidiSequence import RenderMidiSequence
from .q_objects.TestCase import TestCase

from .logging import Logger as BareLogger
from .js_constants import create_js_constants

import time

import PySide6.QtCore as QtCore

qt_logger = BareLogger("Frontend.Qt")
qt_message_handler_installed = False

def qt_msg_handler(mode, context, message):
    fn = None
    if mode == QtCore.QtMsgType.QtInfoMsg:
        fn = qt_logger.info
    elif mode == QtCore.QtMsgType.QtWarningMsg:
        fn = qt_logger.warning
    elif mode == QtCore.QtMsgType.QtCriticalMsg:
        fn = qt_logger.error
    elif mode == QtCore.QtMsgType.QtFatalMsg:
        fn = qt_logger.error
    else:
        fn = qt_logger.debug

    fn("%s: %s (%s:%d, %s)" % (mode, message, context.file, context.line, context.file))

def install_qt_message_handler():
    global qt_message_handler_installed
    global qt_logger
    # if not qt_message_handler_installed:
    # FIXME
    if False:
        QtCore.qInstallMessageHandler(qt_msg_handler)
        qt_message_handler_installed = True

def register_qml_class(t, name):
    qmlRegisterType(t, "ShoopDaLoop.Python" + name, 1, 0, "Python" + name)

def create_constants_instance(engine):
    return create_js_constants(engine)

def register_shoopdaloop_qml_classes():
    # Register Python classes
    register_qml_class(LoopAudioChannel, 'LoopAudioChannel')
    register_qml_class(LoopMidiChannel, 'LoopMidiChannel')
    register_qml_class(ClickTrackGenerator, 'ClickTrackGenerator')
    register_qml_class(KeyModifiers, 'KeyModifiers')
    register_qml_class(ApplicationMetadata, 'ApplicationMetadata')
    register_qml_class(Logger, 'Logger')
    register_qml_class(LuaEngine, 'LuaEngine')
    register_qml_class(DictTreeModelFactory, 'DictTreeModelFactory')
    register_qml_class(ControlHandler, 'ControlHandler')
    register_qml_class(ControlInterface, 'ControlInterface')
    register_qml_class(MidiControlPort, 'MidiControlPort')
    register_qml_class(SettingsIO, 'SettingsIO')
    register_qml_class(TestScreenGrabber, 'TestScreenGrabber')
    register_qml_class(RenderMidiSequence, 'RenderMidiSequence')
    register_qml_class(TestCase, 'TestCase')

    qmlRegisterSingletonType("ShoopConstants", 1, 0, "ShoopConstants", create_constants_instance)
    install_qt_message_handler()

def create_and_populate_root_context_with_engine_addr(engine_addr):
    from shiboken6 import Shiboken, getCppPointer
    from PySide6.QtQml import QQmlApplicationEngine
    engine = Shiboken.wrapInstance(engine_addr, QQmlApplicationEngine)
    return create_and_populate_root_context(engine)

def create_and_populate_root_context(engine):
    def create_component(path):
        comp = QQmlComponent(engine, QUrl.fromLocalFile(path))
        while comp.status() == QQmlComponent.Loading:
            time.sleep(0.05)
        if comp.status() != QQmlComponent.Ready:
            raise Exception('Failed to load {}: {}'.format(path, str(comp.errorString())))
        return comp

    # QML instantiations
    registries_comp = create_component(shoop_qml_dir + '/AppRegistries.qml')
    registries = registries_comp.create()

    items = {
        'click_track_generator': ClickTrackGenerator(parent=engine),
        'key_modifiers': KeyModifiers(parent=engine),
        'app_metadata': ApplicationMetadata(parent=engine),
        'default_logger': Logger(),
        'tree_model_factory': DictTreeModelFactory(parent=engine),
        # 'global_args': global_args,
        'settings_io': SettingsIO(parent=engine),
        'registries': registries,
        'screen_grabber': TestScreenGrabber(weak_engine=weakref.ref(engine), parent=engine)
    }

    # if additional_items:
    #     for key, item in additional_items.items():
    #         items[key] = item

    items['default_logger'].name = 'Frontend.Qml'

    items['app_metadata'].version_string = pkg_version

    for key, item in items.items():
        engine.rootContext().setContextProperty(key, item)

    return items

