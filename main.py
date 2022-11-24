#!/usr/bin/python

import sys

from PyQt6.QtGui import QGuiApplication
from PyQt6.QtQml import QQmlApplicationEngine, qmlRegisterType
from PyQt6.QtCore import QTimer, QMetaObject, Qt

from lib.q_objects.LooperState import LooperState
from lib.q_objects.NChannelAbstractLooperManager import NChannelAbstractLooperManager
from lib.q_objects.DryWetPairAbstractLooperManager import DryWetPairAbstractLooperManager
from lib.q_objects.BackendManager import BackendManager
from lib.q_objects.ClickTrackGenerator import ClickTrackGenerator
from lib.q_objects.MIDIControlManager import MIDIControlManager
from lib.q_objects.MIDIControlLink import MIDIControlLink
from lib.q_objects.BackendManager import BackendManager
from lib.q_objects.PortManager import PortManager

from lib.JackSession import JackSession
from lib.port_loop_mappings import get_port_loop_mappings
from lib.port_input_remap_monitor import start_port_input_remapping_monitor
from third_party.pyjacklib import jacklib

from collections import OrderedDict

import threading

import importlib
pynsm = importlib.import_module('third_party.new-session-manager.extras.pynsm.nsmclient')

from pprint import *

import signal
import psutil
import os
import time
import random
import string

engine = None
nsm_client = None
exitcode = 0
g_backend_mgr = None

class NSMQGuiApplication(QGuiApplication):
    def __init__(self, argv):
        super(NSMQGuiApplication, self).__init__(argv)
        self.nsm_client = None
    
    def exit(self, retcode):
        if nsm_client:
            print("Requesting exit from NSM.")
            nsm_client.serverSendExitToSelf()
        else:
            print("Exiting.")
            self.really_exit()

    def really_exit(self, retcode):
        super.exit(retcode)

app = NSMQGuiApplication(sys.argv)

def exit_handler():
    current_process = psutil.Process()
    children = current_process.children(recursive=False)
    for child in children:
        print('Send signal {} => {}'.format(sig, child.pid))
        os.kill(child.pid, sig)
    if engine:
        QMetaObject.invokeMethod(engine, 'quit')

# Ensure that we forward any terminating signals to our child
# processes
def exit_signal_handler(sig, frame):
    print('Got signal {}.'.format(sig))    
    print('Exiting due to signal.')
    exit_handler()

def exit_nsm_handler():
    print('Exiting due to NSM request.')
    if g_backend_mgr:
        g_backend_mgr.terminate()
    exit_handler()
    sys.exit(0)

def load_session_handler(filename):
    raise NotImplementedError()

def save_session_handler(filename):
    raise NotImplementedError()

signal.signal(signal.SIGINT, exit_signal_handler)
signal.signal(signal.SIGQUIT, exit_signal_handler)
signal.signal(signal.SIGTERM, exit_signal_handler)

script_pwd = os.path.dirname(__file__)

mappings = get_port_loop_mappings(
        8,
        6,
        ['l', 'r']
    )

title = 'ShoopDaLoop'
try:
    nsm_client = pynsm.NSMClient(
        prettyName = title,
        supportsSaveStatus = True,
        saveCallback = lambda path, session, client: save_session_handler(path),
        openOrNewCallback = lambda path, session, client: load_session_handler(path),
        exitProgramCallback = lambda path, session, client: exit_nsm_handler(),
        loggingLevel = 'info'
    )
    title = nsm_client.ourClientNameUnderNSM
except pynsm.NSMNotRunningError as e:
    pass

with BackendManager(
    mappings['port_name_pairs'],
    mappings['loops_to_ports'],
    mappings['loops_hard_sync'],
    mappings['loops_soft_sync'],
    60.0,
    title,
    0.03, # 30Hz updates
    app
) as backend_mgr:
    g_backend_mgr = backend_mgr
    jack_client = backend_mgr.jack_client

    click_track_generator = ClickTrackGenerator(app)
    midi_control_mgr = MIDIControlManager(app, jack_client, backend_mgr)

    start_port_input_remapping_monitor(
        jack_client,
        [backend_mgr.get_jack_input_port(i) for i in range(len(mappings['port_name_pairs']))],
        mappings['port_input_remaps_if_disconnected'],
        backend_mgr.remap_port_input,
        backend_mgr.reset_port_input_remap
    )

    qmlRegisterType(NChannelAbstractLooperManager, 'NChannelAbstractLooperManager', 1, 0, 'NChannelAbstractLooperManager')
    qmlRegisterType(DryWetPairAbstractLooperManager, 'DryWetPairAbstractLooperManager', 1, 0, 'DryWetPairAbstractLooperManager')
    qmlRegisterType(BackendManager, 'BackendManager', 1, 0, 'BackendManager')
    qmlRegisterType(LooperState, 'LooperState', 1, 0, 'LooperState')
    qmlRegisterType(ClickTrackGenerator, 'ClickTrackGenerator', 1, 0, 'ClickTrackGenerator')
    qmlRegisterType(MIDIControlManager, 'MIDIControlManager', 1, 0, 'MIDIControlManager')
    qmlRegisterType(PortManager, 'PortManager', 1, 0, 'PortManager')

    engine = QQmlApplicationEngine()
    engine.rootContext().setContextProperty("backend_manager", backend_mgr)
    engine.rootContext().setContextProperty("click_track_generator", click_track_generator)
    engine.rootContext().setContextProperty("midi_control_manager", midi_control_mgr)
    engine.quit.connect(app.quit)
    engine.load('main.qml')

    exitcode = 0

    # The following ensures the Python interpreter has a chance to run, which
    # would not happen otherwise once the Qt event loop starts - and this 
    # is necessary for allowing the signal handlers to do their job.
    # It's a hack but it works...
    # NOTE: later added NSM react as another goal for this task.
    def tick():
        if nsm_client:
                nsm_client.reactToMessage
    timer = QTimer()
    timer.start(100)
    timer.timeout.connect(tick)

    print("Executing app.")
    exitcode = app.exec()

sys.exit(exitcode)
