from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot

# TODO make settable
loop_channels = 2
loop_min_len = 60.0

class SLGlobalManager(QObject):
    # State change notifications
    slLooperCountChanged = pyqtSignal(int)
    desiredLooperCountChanged = pyqtSignal(int)

    # Signal used to send OSC messages to SooperLooper
    sendOscExpectResponse = pyqtSignal(list, str)
    sendOsc = pyqtSignal(list)

    def __init__(self, parent=None):
        super(SLGlobalManager, self).__init__(parent)
        self._desired_looper_count = None
        self._sl_looper_count = None
        self._last_requested_loop_count = None

    @pyqtSlot()
    def start_sync(self):
        # Register for change updates
        self.sendOscExpectResponse.emit(['/register'], '/hostinfo')

        # Request the most recent state once to have a good starting point
        self.sendOscExpectResponse.emit(['/ping'], '/hostinfo')

        # Set some global parameters
        # TODO: also check this once in a while
        self.sendOsc.emit(['/set', 'sync_source', 1]) # Sync to loop 1

    @pyqtSlot()
    def add_needed_loops(self):
        if self._desired_looper_count != None and \
           self._sl_looper_count != None:
            amount_needed = self._desired_looper_count - self._sl_looper_count
            if self._last_requested_loop_count != self._desired_looper_count:
                for idx in range(amount_needed):
                    self.sendOsc.emit(['/loop_add', loop_channels, loop_min_len])
                self._last_requested_loop_count = self._desired_looper_count

    @pyqtProperty(int, notify=slLooperCountChanged)
    def sl_looper_count(self):
        return self._sl_looper_count

    @sl_looper_count.setter
    def sl_looper_count(self, c):
        if self._sl_looper_count != c:
            self._sl_looper_count = c
            self.slLooperCountChanged.emit(c)

    @pyqtProperty(int, notify=desiredLooperCountChanged)
    def desired_looper_count(self):
        return self._desired_looper_count

    @desired_looper_count.setter
    def desired_looper_count(self, c):
        if self._desired_looper_count != c:
            self._desired_looper_count = c
            self.desiredLooperCountChanged.emit(c)
            self.add_needed_loops()

    # TODO: don't know why I need this, setter doesn't work directly from QML
    @pyqtSlot(int)
    def set_desired_looper_count(self, c):
        self.desired_looper_count = c

    @pyqtSlot(list)
    def onOscReceived(self, msg):
        if msg[0] == '/hostinfo' and len(msg) == 4:
            self._sl_looper_count = int(msg[3])
            self.add_needed_loops()

    @pyqtSlot(QObject)
    def connect_osc_link(self, link):
        link.received.connect(self.onOscReceived)
        self.sendOscExpectResponse.connect(link.send_expect_response)
        self.sendOsc.connect(link.send)
        self.start_sync()
