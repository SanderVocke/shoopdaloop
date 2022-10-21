from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot
import tempfile
import os
import time
import tarfile
import shutil
import wave

# TODO make settable
loop_channels = 2
loop_min_len = 60.0

class SLGlobalManager(QObject):
    # State change notifications
    slLooperCountChanged = pyqtSignal(int)
    desiredLooperCountChanged = pyqtSignal(int)
    allLoopsReadyChanged = pyqtSignal(bool)

    # Signal used to send OSC messages to SooperLooper
    sendOscExpectResponse = pyqtSignal(list, str)
    sendOsc = pyqtSignal(list)

    def __init__(self, parent=None):
        super(SLGlobalManager, self).__init__(parent)
        self._desired_looper_count = None
        self._sl_looper_count = None
        self._last_requested_loop_count = None
        self._all_loops_ready = False

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
            current_amount = self._last_requested_loop_count if self._last_requested_loop_count else self.sl_looper_count
            amount_needed = self._desired_looper_count - current_amount
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
    
    @pyqtProperty(bool, notify=allLoopsReadyChanged)
    def all_loops_ready(self):
        return self._all_loops_ready
    
    @all_loops_ready.setter
    def all_loops_ready(self, c):
        if self._all_loops_ready != c:
            self._all_loops_ready = c
            self.allLoopsReadyChanged.emit(c)
            if c:
                print("All {} SooperLooper loops are ready.".format(self.desired_looper_count))

    # TODO: don't know why I need this, setter doesn't work directly from QML
    @pyqtSlot(int)
    def set_desired_looper_count(self, c):
        self.desired_looper_count = c

    @pyqtSlot(list)
    def onOscReceived(self, msg):
        if msg[0] == '/hostinfo' and len(msg) == 4:
            self._sl_looper_count = int(msg[3])
            self.all_loops_ready = (self.desired_looper_count != None) and \
                (self.desired_looper_count > 0 and self.sl_looper_count >= self.desired_looper_count)
            self.add_needed_loops()

    @pyqtSlot(QObject)
    def connect_osc_link(self, link):
        link.received.connect(self.onOscReceived)
        self.sendOscExpectResponse.connect(link.send_expect_response)
        self.sendOsc.connect(link.send)
        self.start_sync()

    # Save the SooperLooper session, combined with a string
    # representing ShoopDaLoop session data, into a file.
    @pyqtSlot(str, list, bool, str)
    def save_session(self, shoopdaloop_state_serialized, loop_managers, store_audio, filename):
        folder = tempfile.mkdtemp()
        sl_session_filename = folder + '/sooperlooper_session'
        shoop_session_filename = folder + '/shoopdaloop_session'
        tar_filename = folder + '/session.tar'
        self.sendOscExpectResponse.emit(['/save_session', sl_session_filename], '/save_session_error')
        start_t = time.monotonic()
        while time.monotonic() - start_t < 5.0 and not os.path.isfile(sl_session_filename):
            time.sleep(0.05)
        if not os.path.isfile(sl_session_filename):
            raise Exception('SooperLooper did not save its session.')

        with open(shoop_session_filename, 'w') as file:
            file.write(shoopdaloop_state_serialized)

        include_wavs = []
        if store_audio:
            for idx,manager in enumerate(loop_managers):
                if manager.length > 0.0:
                    wav_filename = folder + '/' + str(idx) + '.wav'
                    manager.doSaveWav(wav_filename)
                    include_wavs.append(wav_filename)

        # Now combine into a tarball
        with tarfile.open(tar_filename, 'w') as file:
            file.add(sl_session_filename, os.path.basename(sl_session_filename))
            file.add(shoop_session_filename, os.path.basename(shoop_session_filename))
            for wav in include_wavs:
                file.add(wav, os.path.basename(wav))

        # Save as a session file
        shutil.move(tar_filename, filename)

    # Load the SooperLooper session from a .shl file, returning the
    # ShoopDaLoop-specific state data.
    @pyqtSlot(list, QObject, str, result=str)
    def load_session(self, loop_managers, osc_link, filename):
        folder = tempfile.mkdtemp()
        session_data = None
        with tarfile.open(filename, 'r') as file:
            file.extractall(folder)
        self.sendOscExpectResponse.emit(['/load_session', folder + '/sooperlooper_session'], '/load_session_error')

        # We want to wait until session loading is finished before we start loading any loops.
        # During session load, SooperLooper will delete all loops (until loop count is 0) and then
        # set the loop count to the new amount. We can detect this sequence.
        # TODO: implement the above instead of a "dumb" wait.
        time.sleep(5.0)

        # For each loop, either load #.wav or default.wav if absent
        for idx,manager in enumerate(loop_managers):
            wav_filename = folder + '/' + str(idx) + ".wav"
            if os.path.isfile(wav_filename):
                manager.doLoadWav(wav_filename)

        with open(folder + '/shoopdaloop_session', 'r') as file:
            session_data = file.read()
        return session_data

