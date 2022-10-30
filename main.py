import sys

from PyQt6.QtGui import QGuiApplication
from PyQt6.QtQml import QQmlApplicationEngine, qmlRegisterType
from PyQt6.QtCore import QTimer

from lib.q_objects.SLLooperManager import SLLooperManager
from lib.q_objects.LooperManager import LooperManager
from lib.q_objects.SLFXLooperPairManager import SLFXLooperPairManager
from lib.q_objects.SLGlobalManager import SLGlobalManager
from lib.q_objects.SooperLooperOSCLink import SooperLooperOSCLink
from lib.q_objects.ClickTrackGenerator import ClickTrackGenerator
from lib.q_objects.MIDIControlManager import MIDIControlManager
from lib.q_objects.MIDIControlLink import MIDIControlLink

from lib.JackSession import JackSession
from lib.SooperLooperSession import SooperLooperSession

from third_party.pyjacklib import jacklib

import signal
import psutil
import os
import time
import random
import string

# Ensure that we forward any terminating signals to our child
# processes
def exit_handler(sig, frame):
    print('Got signal {}.'.format(sig))
    current_process = psutil.Process()
    children = current_process.children(recursive=False)
    for child in children:
        print('Send signal {} => {}'.format(sig, child.pid))
        os.kill(child.pid, sig)
    print('Exiting.')
    sys.exit(0)

signal.signal(signal.SIGINT, exit_handler)
signal.signal(signal.SIGQUIT, exit_handler)
signal.signal(signal.SIGTERM, exit_handler)

script_pwd = os.path.dirname(__file__)

with JackSession('ShoopDaLoop-control') as jack_session:
    jack = jack_session[0]
    jack_client = jack_session[1]
    with SooperLooperSession(1, 6, 2, 9951, 'ShoopDaLoop', jack, jack_client):
        app = QGuiApplication(sys.argv)

        link = SooperLooperOSCLink(None, '0.0.0.0', 9951, '0.0.0.0', 9952)
        click_track_generator = ClickTrackGenerator()
        global_mgr = SLGlobalManager(None)
        global_mgr.connect_osc_link(link)
        midi_control_mgr = MIDIControlManager()
        midi_control_link = MIDIControlLink(None, 'ctl_out', 'ctl_in', jack_client, jack)
        midi_control_mgr.sendMidi.connect(midi_control_link.send)
        midi_control_link.received.connect(midi_control_mgr.receiveMidi)

        qmlRegisterType(SLLooperManager, 'SLLooperManager', 1, 0, 'SLLooperManager')
        qmlRegisterType(SLFXLooperPairManager, 'SLFXLooperPairManager', 1, 0, 'SLFXLooperPairManager')
        qmlRegisterType(SLGlobalManager, 'SLGlobalManager', 1, 0, 'SLGlobalManager')
        qmlRegisterType(LooperManager, 'LooperManager', 1, 0, 'LooperManager')
        qmlRegisterType(SooperLooperOSCLink, 'SooperLooperOSCLink', 1, 0, 'SooperLooperOSCLink')
        qmlRegisterType(ClickTrackGenerator, 'ClickTrackGenerator', 1, 0, 'ClickTrackGenerator')
        qmlRegisterType(MIDIControlManager, 'MIDIControlManager', 1, 0, 'MIDIControlManager')

        engine = QQmlApplicationEngine()
        engine.rootContext().setContextProperty("osc_link", link)
        engine.rootContext().setContextProperty("sl_global_manager", global_mgr)
        engine.rootContext().setContextProperty("click_track_generator", click_track_generator)
        engine.rootContext().setContextProperty("midi_control_manager", midi_control_mgr)
        engine.quit.connect(app.quit)
        engine.load('main.qml')

        exitcode = 0

        # This hacky solution ensures that the Python interpreter has a chance
        # to run every 100ms, which e.g. allows the signal handlers to work.
        timer = QTimer()
        timer.start(100)
        timer.timeout.connect(lambda: None)

        exitcode = app.exec()

sys.exit(exitcode)
