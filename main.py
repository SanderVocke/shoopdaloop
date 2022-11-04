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

    # Set up port loop mappings.
    # 4 ports per track: L dry, R dry, L wet, R wet
    n_tracks = 8
    loops_per_track = 6    
    port_name_pairs = [
        ('master_loop_in_l', 'master_loop_send_l'),
        ('master_loop_in_r', 'master_loop_send_r'),
        ('master_loop_return_l', 'master_loop_out_l'),
        ('master_loop_return_r', 'master_loop_out_r'),
    ]
    for track_idx in range(n_tracks):
        port_name_pairs.append(
            ('track_{}_in_l'.format(track_idx + 1),
             'track_{}_send_l'.format(track_idx + 1))
        )
        port_name_pairs.append(
            ('track_{}_in_r'.format(track_idx + 1),
             'track_{}_send_r'.format(track_idx + 1))
        )
        port_name_pairs.append(
            ('track_{}_return_l'.format(track_idx + 1),
             'track_{}_out_l'.format(track_idx + 1))
        )
        port_name_pairs.append(
            ('track_{}_return_r'.format(track_idx + 1),
             'track_{}_out_r'.format(track_idx + 1))
        )
    
    # loop ordering: 4 loops per "logical loop".
    # L dry, R dry, L wet, R wet
    loops_to_ports = [0, 1, 2, 3]
    loops_soft_sync = [0, 1, 2, 3] # Master loop is not synced (TODO)
    loops_hard_sync = [0, 0, 1, 1] # Hard-link channels together
    for t in range(n_tracks):
        for l in range(loops_per_track):
            base = 4 + t*4
            loops_to_ports += [base, base + 1, base + 2, base + 3]
            loops_soft_sync += [0, 0, 0, 0]
            loops_hard_sync += [base, base, base + 1, base + 1]
    
    # Start the back-end
    backend_mgr = BackendManager(
        port_name_pairs,
        loops_to_ports,
        loops_hard_sync,
        loops_soft_sync,
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
