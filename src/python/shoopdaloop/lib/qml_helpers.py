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

from .q_objects.Logger import Logger
from .q_objects.DictTreeModel import DictTreeModelFactory

from .logging import Logger as BareLogger

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


def register_shoopdaloop_qml_classes():
    # Register Python classes
    register_qml_class(Logger, 'Logger')
    register_qml_class(DictTreeModelFactory, 'DictTreeModelFactory')
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
        'default_logger': Logger(),
        'tree_model_factory': DictTreeModelFactory(parent=engine),
        'registries': registries,
    }

    items['default_logger'].name = 'Frontend.Qml'

    for key, item in items.items():
        engine.rootContext().setContextProperty(key, item)

    return items

