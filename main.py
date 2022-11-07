#!/usr/bin/python

import sys

from PyQt6.QtGui import QGuiApplication
from PyQt6.QtQml import QQmlApplicationEngine, qmlRegisterType
from PyQt6.QtCore import QTimer, QMetaObject, Qt

from lib.q_objects.BasicLooperManager import BasicLooperManager
from lib.q_objects.NChannelAbstractLooperManager import NChannelAbstractLooperManager
from lib.q_objects.DryWetPairAbstractLooperManager import DryWetPairAbstractLooperManager
from lib.q_objects.BackendManager import BackendManager
from lib.q_objects.ClickTrackGenerator import ClickTrackGenerator
from lib.q_objects.MIDIControlManager import MIDIControlManager
from lib.q_objects.MIDIControlLink import MIDIControlLink
from lib.q_objects.BackendManager import BackendManager

from lib.JackSession import JackSession
from lib.port_loop_mappings import get_port_loop_mappings
from third_party.pyjacklib import jacklib

from collections import OrderedDict

from pprint import *

import signal
import psutil
import os
import time
import random
import string

engine = None

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
    if engine:
        QMetaObject.invokeMethod(engine, 'quit')

signal.signal(signal.SIGINT, exit_handler)
signal.signal(signal.SIGQUIT, exit_handler)
signal.signal(signal.SIGTERM, exit_handler)

script_pwd = os.path.dirname(__file__)

mappings = get_port_loop_mappings(
        8,
        6,
        ['l', 'r']
    )

with JackSession('ShoopDaLoop-control') as jack_session:
    jack = jack_session[0]
    jack_client = jack_session[1]

    app = QGuiApplication(sys.argv)

    with BackendManager(
        mappings['port_name_pairs'],
        mappings['loops_to_ports'],
        mappings['loops_hard_sync'],
        mappings['loops_soft_sync'],
        60.0,
        'ShoopDaLoop-backend',
        0.03, # 30Hz updates
        app
    ) as backend_mgr:
        click_track_generator = ClickTrackGenerator(app)
        midi_control_mgr = MIDIControlManager(app, jack_client, jack)

        qmlRegisterType(NChannelAbstractLooperManager, 'NChannelAbstractLooperManager', 1, 0, 'NChannelAbstractLooperManager')
        qmlRegisterType(DryWetPairAbstractLooperManager, 'DryWetPairAbstractLooperManager', 1, 0, 'DryWetPairAbstractLooperManager')
        qmlRegisterType(BackendManager, 'BackendManager', 1, 0, 'BackendManager')
        qmlRegisterType(BasicLooperManager, 'BasicLooperManager', 1, 0, 'BasicLooperManager')
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
        # timer.start(100)
        timer.timeout.connect(lambda: None)

        exitcode = app.exec()

sys.exit(exitcode)
