#!/usr/bin/python
import sys
import os

from PySide6 import QtCore, QtGui, QtQml

script_dir = os.path.dirname(__file__)

from PySide6.QtCore import QUrl, QCoreApplication, Qt
from PySide6.QtGui import QGuiApplication
from PySide6.QtQml import QQmlApplicationEngine

import subprocess

subproc = None

if os.environ['XDG_SESSION_TYPE'] == 'x11':
    print("Detected X11 session, using xcb_egl integration")
    os.environ['QT_XCB_GL_INTEGRATION'] = 'xcb_egl'

QCoreApplication.setAttribute(Qt.AA_ShareOpenGLContexts, True)

class Helper(QtCore.QObject):
    def __init__(self, parent=None):
        super(Helper, self).__init__(parent)
    
    @QtCore.Slot(str, str)
    def set_env(self, key, value):
        os.environ[key] = value
    
    @QtCore.Slot()
    def initialized_callback(self):
        global subproc
        print("Initialized callback called, running client")
        subproc = subprocess.Popen(['carla'])
        
QtQml.qmlRegisterType(Helper, "Helper", 1, 0, "Helper")

app = QGuiApplication(sys.argv)

appEngine = QQmlApplicationEngine(os.path.join(script_dir, 'main.qml'))

exit(app.exec())