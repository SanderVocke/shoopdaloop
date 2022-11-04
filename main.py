import sys

from PyQt6.QtGui import QGuiApplication
from PyQt6.QtQml import QQmlApplicationEngine, qmlRegisterType
from PyQt6.QtCore import QTimer

from lib.q_objects.LooperManager import LooperManager
from lib.q_objects.BackendLooperManager import BackendLooperManager
from lib.q_objects.BackendFXLooperPairManager import BackendFXLooperPairManager
from lib.q_objects.BackendManager import BackendManager
from lib.q_objects.ClickTrackGenerator import ClickTrackGenerator
from lib.q_objects.MIDIControlManager import MIDIControlManager
from lib.q_objects.MIDIControlLink import MIDIControlLink
from lib.q_objects.BackendManager import BackendManager

from lib.JackSession import JackSession
from lib.SooperLooperSession import SooperLooperSession
from third_party.pyjacklib import jacklib

from collections import OrderedDict

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

    app = QGuiApplication(sys.argv)

    click_track_generator = ClickTrackGenerator()
    midi_control_mgr = MIDIControlManager(None, jack_client, jack)

    # Set up port name mappings
    port_names_to_loop_amounts = OrderedDict()
    port_names_to_loop_amounts[('master_loop_in_l', 'master_loop_out_l')] = 1
    port_names_to_loop_amounts[('master_loop_in_r', 'master_loop_out_r')] = 1
    n_tracks = 8
    loops_per_track = 6
    for track_idx in range(n_tracks):
        port_names_to_loop_amounts[(
            'loop_{}_in_l'.format(track_idx + 1),
            'loop_{}_out_l'.format(track_idx + 1),
        )] = loops_per_track
        port_names_to_loop_amounts[(
            'loop_{}_in_r'.format(track_idx + 1),
            'loop_{}_out_r'.format(track_idx + 1),
        )] = loops_per_track
    
    # Start the back-end
    backend_mgr = BackendManager(
        port_names_to_loop_amounts,
        2,
        60.0,
        'ShoopDaLoop-backend',
        0.03 # About 30Hz updates
    )

    qmlRegisterType(BackendLooperManager, 'BackendLooperManager', 1, 0, 'BackendLooperManager')
    qmlRegisterType(BackendFXLooperPairManager, 'BackendFXLooperPairManager', 1, 0, 'BackendFXLooperPairManager')
    qmlRegisterType(BackendManager, 'BackendManager', 1, 0, 'BackendManager')
    qmlRegisterType(LooperManager, 'LooperManager', 1, 0, 'LooperManager')
    qmlRegisterType(ClickTrackGenerator, 'ClickTrackGenerator', 1, 0, 'ClickTrackGenerator')
    qmlRegisterType(MIDIControlManager, 'MIDIControlManager', 1, 0, 'MIDIControlManager')

    engine = QQmlApplicationEngine()
    engine.rootContext().setContextProperty("backend_manager", backend_mgr)
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
