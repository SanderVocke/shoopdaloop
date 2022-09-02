import sys
import time

from PyQt6.QtGui import QGuiApplication
from PyQt6.QtQml import QQmlApplicationEngine, qmlRegisterType
from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, QTimer

from q_objects.SLLooperState import SLLooperState
from q_objects.SLLooperManager import SLLooperManager
from q_objects.SooperLooperOSCLink import SooperLooperOSCLink

import pprint

app = QGuiApplication(sys.argv)

link = SooperLooperOSCLink(None, '0.0.0.0', 9951, '0.0.0.0', 9952)

qmlRegisterType(SLLooperState, 'SLLooperState', 1, 0, 'SLLooperState')
qmlRegisterType(SLLooperManager, 'SLLooperManager', 1, 0, 'SLLooperManager')
qmlRegisterType(SooperLooperOSCLink, 'SooperLooperOSCLink', 1, 0, 'SooperLooperOSCLink')

engine = QQmlApplicationEngine()
engine.rootContext().setContextProperty("osc_link", link)
engine.quit.connect(app.quit)
engine.load('main.qml')

sys.exit(app.exec())