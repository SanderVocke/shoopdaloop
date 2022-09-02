from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot

class LoopState(QObject):
    lengthChanged = pyqtSignal(float)
    posChanged = pyqtSignal(float)
    slLoopIndexChanged = pyqtSignal(int)

    def __init__(self, parent=None):
        super(LoopState, self).__init__(parent)
        self._length = 1.0
        self._pos = 0.0
        self._sl_loop_index = 0
    
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
    
    @pyqtProperty(int, notify=slLoopIndexChanged)
    def sl_loop_index(self):
        return self._sl_loop_index
    
    @sl_loop_index.setter
    def sl_loop_index(self, idx):
        if self._sl_loop_index != idx:
            self._sl_loop_index = idx
            self.slLoopIndexChanged.emit(idx)
    
    @pyqtSlot()
    def onLoopParameterChanged(self):#, loop_idx, control, value):
        print('hello')
        #if loop_idx == self._sl_loop_index:
        #    if control == 'loop_pos':
        #        self.pos = float(value)
        #    elif control == 'loop_len':
        #        self.length = float(value)
        #print('Loop {} received an update! {} {}'.format(loop_idx, control, value))