import sys
import time

from PyQt6.QtGui import QGuiApplication
from PyQt6.QtQml import QQmlApplicationEngine
from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, QTimer

from q_objects.LoopState import LoopState
from q_objects.SooperLooperOSCLink import SooperLooperOSCLink

import pprint

#def update_loop(mgr):
#    t = time.monotonic()
#    mgr.pos = t % mgr.length

app = QGuiApplication(sys.argv)

link = SooperLooperOSCLink(None, '0.0.0.0', 9951, '0.0.0.0', 9952)
link.received_loop_param.connect(lambda idx, control, val: print("Loop {}: {} = {}".format(idx, control, val)))

while True:
    input()
    link.request_loop_parameter(0, 'loop_pos')

engine = QQmlApplicationEngine()
#loop_mgr = LoopManager()
#loop_mgr.length = 10.0
#engine.rootContext().setContextProperty("loop_manager", loop_mgr)
#timer = QTimer(interval=100)
#timer.timeout.connect(lambda: update_loop(loop_mgr))
#timer.start()
engine.quit.connect(app.quit)
engine.load('main.qml')

sys.exit(app.exec())