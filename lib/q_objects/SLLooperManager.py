from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot
import re

class SLLooperManager(QObject):

    # State change notifications to the looper
    lengthChanged = pyqtSignal(float)
    posChanged = pyqtSignal(float)
    slLooperIndexChanged = pyqtSignal(int)
    slLooperCountChanged = pyqtSignal(int)
    connectedChanged = pyqtSignal(bool)
    stateChanged = pyqtSignal(str)

    # Signal used to send OSC messages to SooperLooper
    sendOscExpectResponse = pyqtSignal(list, str)
    sendOsc = pyqtSignal(list)

    def __init__(self, parent=None, sl_looper_index=0):
        super(SLLooperManager, self).__init__(parent)
        self._length = 1.0
        self._pos = 0.0
        self._sl_looper_index = sl_looper_index
        self._sl_looper_count = 0
        self._connected = False

    @pyqtSlot()
    def start_sync(self):
        # Register for repeated updates on "continuous" signals
        for ctl in ['loop_len', 'loop_pos']:
            self.sendOscExpectResponse.emit(['/sl/{}/register_auto_update'.format(self._sl_looper_index), ctl, 100], '/sl/{}/get'.format(self._sl_looper_index))
        
        # Register for change updates
        for ctl in ['state', 'is_soloed']:
            self.sendOscExpectResponse.emit(['/sl/{}/register_update'.format(self._sl_looper_index), ctl], '/sl/{}/get'.format(self._sl_looper_index))
        # Also for loop count, url, version
        self.sendOscExpectResponse.emit(['/register'], '/hostinfo')

        # Request the most recent state once to have a good starting point
        for ctl in ['loop_len', 'loop_pos', 'state', 'is_soloed']:
            self.sendOscExpectResponse.emit(['/sl/{}/get'.format(self._sl_looper_index), ctl], '/sl/{}/get'.format(self._sl_looper_index))
        # Also for loop count, url, version
        self.sendOscExpectResponse.emit(['/ping'], '/hostinfo')

    @pyqtProperty(int, notify=slLooperIndexChanged)
    def sl_looper_index(self):
        return self._sl_looper_index

    @sl_looper_index.setter
    def sl_looper_index(self, i):
        if self._sl_looper_index != i:
            self._sl_looper_index = i
            self.slLooperIndexChanged.emit(i)
            self.updateConnected()
            self.start_sync()

    @pyqtProperty(int, notify=slLooperCountChanged)
    def sl_looper_count(self):
        return self._sl_looper_count
    
    @sl_looper_count.setter
    def sl_looper_count(self, c):
        if self._sl_looper_count != c:
            self._sl_looper_count = c
            self.slLooperCountChanged.emit(c)
            self.updateConnected()
    
    @pyqtProperty(bool, notify=connectedChanged)
    def connected(self):
        return self._connected
    
    @connected.setter
    def connected(self, a):
        if self._connected != a:
            self._connected = a
            self.connectedChanged.emit(a)

    @pyqtSlot()
    def updateConnected(self):
        self.connected = bool(self._sl_looper_count > self._sl_looper_index)
    
    @pyqtSlot(list)
    def onOscReceived(self, msg):
        # Check if it is a changed loop parameter.
        maybe_loop_param = re.match(r'/sl/([0-9]+)/get', msg[0])
        if maybe_loop_param and len(msg) == 4:
            loop_idx = int(msg[1])
            control = str(msg[2])
            value = str(msg[3])
            if loop_idx == self._sl_looper_index:
                if control == 'loop_pos':
                    self.posChanged.emit(float(value))
                elif control == 'loop_len':
                    self.lengthChanged.emit(float(value))
                elif control == 'state':
                    self.stateChanged.emit(value)
        elif msg[0] == '/hostinfo' and len(msg) == 4:
            self.sl_looper_count = int(msg[3])
    
    @pyqtSlot()
    def doTrigger(self):
        self.sendOsc.emit(['/sl/{}/hit'.format(self._sl_looper_index), 'trigger'])

        # msg is e.g. ['/some/path/stuff', 0, 1, 'textarg']
        @pyqtSlot(list)
        def send(self, msg):
            self._snd_queue.put(msg)

        # Use for messages that expect a response from SooperLooper.
        @pyqtSlot(list, str)
        def send_expect_response(self, msg, return_path):
            self.send(msg + ['osc.udp://{}:{}/'.format(self._rcv_ip, self._rcv_port), return_path])

    @pyqtSlot(QObject)
    def connect_osc_link(self, link):
        self.sendOscExpectResponse.connect(link.send_expect_response)
        self.sendOsc.connect(link.send)
