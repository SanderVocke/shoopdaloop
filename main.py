import sys
import time

from PyQt6.QtGui import QGuiApplication
from PyQt6.QtQml import QQmlApplicationEngine
from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, QTimer

class LoopManager(QObject):
    lengthChanged = pyqtSignal(float)
    posChanged = pyqtSignal(float)

    def __init__(self, parent=None):
        super(LoopManager, self).__init__(parent)
        self._length = 1.0
        self._pos = 0.0
    
    @pyqtProperty(float, notify=lengthChanged)
    def length(self):
        return self._length
    
    @length.setter
    def length(self, l):
        if self._length != l:
            self._length = l
            self.lengthChanged.emit(l)
    
    @pyqtProperty(float, notify=posChanged)
    def pos(self):
        return self._pos
    
    @pos.setter
    def pos(self, p):
        if self._pos != p:
            self._pos = p
            self.posChanged.emit(p)

def update_loop(mgr):
    t = time.monotonic()
    mgr.pos = t % mgr.length

app = QGuiApplication(sys.argv)

engine = QQmlApplicationEngine()
loop_mgr = LoopManager()
loop_mgr.length = 10.0
engine.rootContext().setContextProperty("loop_manager", loop_mgr)
timer = QTimer(interval=100)
timer.timeout.connect(lambda: update_loop(loop_mgr))
timer.start()
engine.quit.connect(app.quit)
engine.load('main.qml')



sys.exit(app.exec())