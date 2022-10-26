from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
import re
import time
import os
import wave
import tempfile

from ..LoopState import LoopState
from .LooperManager import LooperManager

# Looper manager for a single loop in Sooperlooper.
class SLLooperManager(LooperManager):

    # State change notifications
    slLooperIndexChanged = pyqtSignal(int)

    # Signal used to send OSC messages to SooperLooper
    sendOscExpectResponse = pyqtSignal(list, str)
    sendOsc = pyqtSignal(list)

    def __init__(self, parent=None, sl_looper_index=0):
        super(SLLooperManager, self).__init__(parent)
        self._sl_looper_index = sl_looper_index
        self.syncChanged.connect(self.update_sl_sync)
        self.passthroughChanged.connect(self.update_sl_passthrough)
        self.volumeChanged.connect(self.update_sl_volume)
        self.panLChanged.connect(self.update_sl_pan_l)
        self.panRChanged.connect(self.update_sl_pan_r)
        self._registered = False

    ######################
    # PROPERTIES
    ######################

    @pyqtProperty(int, notify=slLooperIndexChanged)
    def sl_looper_index(self):
        return self._sl_looper_index

    @sl_looper_index.setter
    def sl_looper_index(self, i):
        if self._sl_looper_index != i:
            self._sl_looper_index = i
            self.slLooperIndexChanged.emit(i)

    ##################
    # SLOTS
    ##################

    @pyqtSlot()
    def start_sync(self):
        if self._registered:
            return

        # print("Registering loop {}".format(self._sl_looper_index))
        
        # Register for repeated updates on "continuous" signals
        for ctl in ['loop_len', 'loop_pos', 'state', 'is_soloed', 'dry', 'wet', 'pan_1', 'pan_2']:
            self.sendOscExpectResponse.emit(['/sl/{}/register_auto_update'.format(self._sl_looper_index), ctl, 100], '/sl/{}/get'.format(self._sl_looper_index))
        
        # Register for change updates
        for ctl in []: # ['state', 'is_soloed']:
            self.sendOscExpectResponse.emit(['/sl/{}/register_update'.format(self._sl_looper_index), ctl], '/sl/{}/get'.format(self._sl_looper_index))
        # Also for loop count, url, version
        self.sendOscExpectResponse.emit(['/register'], '/hostinfo')

        # Request the most recent state once to have a good starting point
        for ctl in ['loop_len', 'loop_pos', 'state', 'is_soloed', 'dry', 'wet', 'pan_1', 'pan_2']:
            self.sendOscExpectResponse.emit(['/sl/{}/get'.format(self._sl_looper_index), ctl], '/sl/{}/get'.format(self._sl_looper_index))
        # Also for loop count, url, version
        self.sendOscExpectResponse.emit(['/ping'], '/hostinfo')

        # Some settings for the loop
        self.sendOsc.emit(['/sl/{}/set'.format(self._sl_looper_index), 'quantize', 1]) # Quantize to cycle
        self.sendOsc.emit(['/sl/{}/set'.format(self._sl_looper_index), 'relative_sync', 0])
        self.sendOsc.emit(['/sl/{}/set'.format(self._sl_looper_index), 'mute_quantized', 1])
        self.sendOsc.emit(['/sl/{}/set'.format(self._sl_looper_index), 'round', 0])
        self.update_sl_sync()
        self.update_sl_passthrough()

        self._registered = True
    
    @pyqtSlot()
    def update_sl_sync(self):
        self.sendOsc.emit(['/set', 'sync_source', 1]) # Sync to loop 1. TODO: shouldn't need to repeat this
        self.sendOsc.emit(['/sl/{}/set'.format(self._sl_looper_index), 'sync', (1 if self.sync else 0)])
        self.sendOsc.emit(['/sl/{}/set'.format(self._sl_looper_index), 'playback_sync', (1 if self.sync else 0)])
    
    @pyqtSlot()
    def update_sl_passthrough(self):
        self.sendOsc.emit(['/sl/{}/set'.format(self._sl_looper_index), 'dry', self.passthrough])
    
    @pyqtSlot()
    def update_sl_volume(self):
        self.sendOsc.emit(['/sl/{}/set'.format(self._sl_looper_index), 'wet', self.volume])
    
    @pyqtSlot()
    def update_sl_pan_l(self):
        self.sendOsc.emit(['/sl/{}/set'.format(self._sl_looper_index), 'pan_1', self.panL])
    
    @pyqtSlot()
    def update_sl_pan_r(self):
        self.sendOsc.emit(['/sl/{}/set'.format(self._sl_looper_index), 'pan_2', self.panR])
    
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
                    self.pos = float(value)
                elif control == 'loop_len':
                    self.length = float(value)
                elif control == 'state':
                    self.state = round(float(value))
    
    @pyqtSlot()
    def doTrigger(self):
        self.sendOsc.emit(['/sl/{}/hit'.format(self._sl_looper_index), 'trigger'])

    @pyqtSlot()
    def doPlay(self):
        if self.state == LoopState.Paused.value:
            self.sendOsc.emit(['/sl/{}/hit'.format(self._sl_looper_index), 'pause'])
        self.doUnmute()

    @pyqtSlot()
    def doPause(self):
        # TODO: if in muted state, it will go to play in the next cycle instead of
        # pause. Not sure what to do about it.
        if self.state != LoopState.Paused.value:
            self.sendOsc.emit(['/sl/{}/hit'.format(self._sl_looper_index), 'pause'])

    @pyqtSlot()
    def doRecord(self):
        if self.state != LoopState.Recording.value:
            # Default recording behavior is to round
            self.sendOsc.emit(['/sl/{}/set'.format(self._sl_looper_index), 'round', 1])
            self.sendOsc.emit(['/sl/{}/hit'.format(self._sl_looper_index), 'record'])

    @pyqtSlot(int, QObject)
    def doRecordNCycles(self, n, master_manager):
        if self.state != LoopState.Recording.value:
            # Recording N cycles is achieved by:
            # - setting round mode (after stop record, will keep recording until synced)
            # - starting to record
            # - scheduling a stop record command somewhere during the Nth cycle.
            self.sendOsc.emit(['/sl/{}/set'.format(self._sl_looper_index), 'round', 1])
            self.sendOsc.emit(['/sl/{}/hit'.format(self._sl_looper_index), 'record'])
            master_manager.schedule_at_loop_pos(master_manager.length * 0.7, n, lambda: self.doStopRecord())

    @pyqtSlot()
    def doStopRecord(self):
        if self.state in [LoopState.Recording.value, LoopState.Inserting.value]:
            self.sendOsc.emit(['/sl/{}/hit'.format(self._sl_looper_index), 'record'])

    @pyqtSlot()
    def doMute(self):
        if self.state != LoopState.Off.value:
            self.sendOsc.emit(['/sl/{}/hit'.format(self._sl_looper_index), 'mute_on'])

    @pyqtSlot()
    def doUnmute(self):
        if self.state != LoopState.Off.value:
            self.sendOsc.emit(['/sl/{}/hit'.format(self._sl_looper_index), 'mute_off'])

    @pyqtSlot()
    def doInsert(self):
        self.sendOsc.emit(['/sl/{}/hit'.format(self._sl_looper_index), 'insert'])

    @pyqtSlot()
    def doReplace(self):
        self.sendOsc.emit(['/sl/{}/hit'.format(self._sl_looper_index), 'replace'])

    @pyqtSlot(str)
    def doLoadWav(self, wav_file):
        # TODO: handle the errors
        self.sendOscExpectResponse.emit(['/sl/{}/load_loop'.format(self._sl_looper_index), wav_file], '/load_loop_error')

    @pyqtSlot()
    def doClear(self):
        # Clearing is possible by loading an empty wav.
        filename = tempfile.mkstemp()[1]
        with wave.open(filename, 'w') as file:
            file.setframerate(48000)
            file.setsampwidth(4)
            file.setnchannels(2)
            file.setnframes(0)
        self.doLoadWav(filename)

    @pyqtSlot(str)
    def doSaveWav(self, wav_file):
        # TODO: handle the errors
        self.sendOscExpectResponse.emit(['/sl/{}/save_loop'.format(self._sl_looper_index), wav_file, 'dummy_format', 'dummy_endian'], '/save_loop_error')
        start_t = time.monotonic()
        while time.monotonic() - start_t < 2.0 and not os.path.isfile(wav_file):
            time.sleep(0.1)
        if not os.path.isfile(wav_file):
            raise Exception("Failed to save wav file for loop.")

    @pyqtSlot(QObject)
    def connect_osc_link(self, link):
        if link is not None:
            link.received.connect(self.onOscReceived)
            self.sendOscExpectResponse.connect(link.send_expect_response)
            self.sendOsc.connect(link.send)
    
    @pyqtSlot(result=str)
    def looper_type(self):
        return "SLLooperManager"
