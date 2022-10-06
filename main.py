import sys

from PyQt6.QtGui import QGuiApplication
from PyQt6.QtQml import QQmlApplicationEngine, qmlRegisterType

from lib.q_objects.SLLooperManager import SLLooperManager
from lib.q_objects.SLGlobalManager import SLGlobalManager
from lib.q_objects.SooperLooperOSCLink import SooperLooperOSCLink
from lib.q_objects.ClickTrackGenerator import ClickTrackGenerator

from lib.JackProxySession import JackProxySession

from third_party.pyjacklib import jacklib

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

jack_server_name = None
exitcode = 0
with JackProxySession(jack_server_name, 2, 2) as jack:
    print("check: {}".format(jacklib.jlib))
    status = jacklib.jack_status_t()
    client = jacklib.client_open("test_client", jacklib.JackNoStartServer | jacklib.JackServerName, status, jack_server_name)
    print(status)

    exitcode = app.exec()

sys.exit(exitcode)
