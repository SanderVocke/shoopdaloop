import sys

from PyQt6.QtGui import QGuiApplication
from PyQt6.QtQml import QQmlApplicationEngine, qmlRegisterType

from lib.q_objects.SLLooperManager import SLLooperManager
from lib.q_objects.SLGlobalManager import SLGlobalManager
from lib.q_objects.SooperLooperOSCLink import SooperLooperOSCLink
from lib.q_objects.ClickTrackGenerator import ClickTrackGenerator

app = QGuiApplication(sys.argv)

link = SooperLooperOSCLink(None, '0.0.0.0', 9951, '0.0.0.0', 9952)
click_track_generator = ClickTrackGenerator()
global_mgr = SLGlobalManager(None)
global_mgr.connect_osc_link(link)

qmlRegisterType(SLLooperManager, 'SLLooperManager', 1, 0, 'SLLooperManager')
qmlRegisterType(SLGlobalManager, 'SLGlobalManager', 1, 0, 'SLGlobalManager')
qmlRegisterType(SooperLooperOSCLink, 'SooperLooperOSCLink', 1, 0, 'SooperLooperOSCLink')
qmlRegisterType(ClickTrackGenerator, 'ClickTrackGenerator', 1, 0, 'ClickTrackGenerator')

engine = QQmlApplicationEngine()
engine.rootContext().setContextProperty("osc_link", link)
engine.rootContext().setContextProperty("sl_global_manager", global_mgr)
engine.rootContext().setContextProperty("click_track_generator", click_track_generator)
engine.quit.connect(app.quit)
engine.load('main.qml')

sys.exit(app.exec())
