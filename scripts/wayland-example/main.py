#!/usr/bin/python
import sys
import os

script_dir = os.path.dirname(__file__)

from PySide6.QtCore import QUrl, QCoreApplication, Qt
from PySide6.QtGui import QGuiApplication
from PySide6.QtQml import QQmlApplicationEngine

QCoreApplication.setAttribute(Qt.AA_ShareOpenGLContexts, True)

app = QGuiApplication(sys.argv)

appEngine = QQmlApplicationEngine(os.path.join(script_dir, 'main.qml'))

exit(app.exec())