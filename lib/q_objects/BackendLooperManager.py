from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
import re
import time
import os
import wave
import tempfile

from ..LoopState import LoopState
from .LooperManager import LooperManager

# Looper manager for a single loop in Sooperlooper.
class BackendLooperManager(LooperManager):
    signalLoopAction = pyqtSignal(int, list) # action_id, args
    loopIdxChanged = pyqtSignal(int)

    def __init__(self, parent=None, loop_idx=0):
        super(BackendLooperManager, self).__init__(parent)
        self._loop_idx = loop_idx
        # self.syncChanged.connect(self.update_sl_sync)
        # self.passthroughChanged.connect(self.update_sl_passthrough)
        # self.volumeChanged.connect(self.update_sl_volume)
        # self.panLChanged.connect(self.update_sl_pan_l)
        # self.panRChanged.connect(self.update_sl_pan_r)

        self.stateChanged.connect(lambda s: print("State -> {}".format(s)))
        self.lengthChanged.connect(lambda s: print("Length -> {}".format(s)))
        self.posChanged.connect(lambda s: print("Position -> {}".format(s)))

    ######################
    # PROPERTIES
    ######################

    @pyqtProperty(int, notify=loopIdxChanged)
    def loop_idx(self):
        return self._loop_idx
    @loop_idx.setter
    def loop_idx(self, l):
        if self._loop_idx != l:
            self._loop_idx = l
            self.loopIdxChanged.emit(l)

    ##################
    # SLOTS
    ##################

    @pyqtSlot(
        int,
        int,
        list,
        list,
        list,
        list,
        list,
        list)
    def update(
        self,
        n_loops,
        n_ports,
        states,
        lengths,
        positions,
        loop_volumes,
        port_volumes,
        port_passthroughs
    ):
        if self._loop_idx >= n_loops:
            raise ValueError("BackendLooperManager with out-of-range loop idx")

        i = self._loop_idx
        self.state = states[i]
        self.length = lengths[i]
        self.pos = positions[i]
        self.volume = loop_volumes[i]
    
    @pyqtSlot(int, list)
    def doLoopAction(self, action_id, args):
        self.signalLoopAction.emit(action_id, args)

    @pyqtSlot(QObject)
    def connect_backend_manager(self, manager):
        self.signalLoopAction.connect(
            lambda act, args: manager.do_loop_action(
                self.loop_idx,
                act,
                args
            )
        )
        manager.update.connect(self.update)

    # @pyqtSlot()
    # def doTrigger(self):
    #     self.sendOsc.emit(['/sl/{}/hit'.format(self._sl_looper_index), 'trigger'])

    # @pyqtSlot()
    # def doPlay(self):
    #     if self.state == LoopState.Paused.value:
    #         self.doTrigger()
    #     elif self.state == LoopState.Muted.value:
    #         self.doUnmute()

    # @pyqtSlot()
    # def doPause(self):
    #     # TODO: if in muted state, it will go to play in the next cycle instead of
    #     # pause. Not sure what to do about it.
    #     if self.state != LoopState.Paused.value:
    #         self.sendOsc.emit(['/sl/{}/hit'.format(self._sl_looper_index), 'pause_on'])

    # @pyqtSlot()
    # def doRecord(self):
    #     if self.state != LoopState.Recording.value:
    #         # Default recording behavior is to round
    #         self.sendOsc.emit(['/sl/{}/set'.format(self._sl_looper_index), 'round', 1])
    #         self.sendOsc.emit(['/sl/{}/hit'.format(self._sl_looper_index), 'record'])

    # @pyqtSlot(int, QObject)
    # def doRecordNCycles(self, n, master_manager):
    #     if self.state != LoopState.Recording.value:
    #         # Recording N cycles is achieved by:
    #         # - setting round mode (after stop record, will keep recording until synced)
    #         # - starting to record
    #         # - scheduling a stop record command somewhere during the Nth cycle.
    #         self.sendOsc.emit(['/sl/{}/set'.format(self._sl_looper_index), 'round', 1])
    #         self.sendOsc.emit(['/sl/{}/hit'.format(self._sl_looper_index), 'record'])
    #         master_manager.schedule_at_loop_pos(master_manager.length * 0.7, n, lambda: self.doStopRecord())

    # @pyqtSlot()
    # def doStopRecord(self):
    #     if self.state in [LoopState.Recording.value, LoopState.Inserting.value]:
    #         self.sendOsc.emit(['/sl/{}/hit'.format(self._sl_looper_index), 'record'])

    # @pyqtSlot()
    # def doMute(self):
    #     if self.state != LoopState.Off.value:
    #         self.sendOsc.emit(['/sl/{}/hit'.format(self._sl_looper_index), 'mute_on'])

    # @pyqtSlot()
    # def doUnmute(self):
    #     if self.state != LoopState.Off.value:
    #         self.sendOsc.emit(['/sl/{}/hit'.format(self._sl_looper_index), 'mute_off'])

    # @pyqtSlot()
    # def doInsert(self):
    #     self.sendOsc.emit(['/sl/{}/hit'.format(self._sl_looper_index), 'insert'])

    # @pyqtSlot()
    # def doReplace(self):
    #     self.sendOsc.emit(['/sl/{}/hit'.format(self._sl_looper_index), 'replace'])

    # @pyqtSlot(str)
    # def doLoadWav(self, wav_file):
    #     # TODO: handle the errors
    #     self.sendOscExpectResponse.emit(['/sl/{}/load_loop'.format(self._sl_looper_index), wav_file], '/load_loop_error')

    # @pyqtSlot()
    # def doClear(self):
    #     # Clearing is possible by loading an empty wav.
    #     filename = tempfile.mkstemp()[1]
    #     with wave.open(filename, 'w') as file:
    #         file.setframerate(48000)
    #         file.setsampwidth(4)
    #         file.setnchannels(2)
    #         file.setnframes(0)
    #     self.doLoadWav(filename)

    # @pyqtSlot(str)
    # def doSaveWav(self, wav_file):
    #     # TODO: handle the errors
    #     self.sendOscExpectResponse.emit(['/sl/{}/save_loop'.format(self._sl_looper_index), wav_file, 'dummy_format', 'dummy_endian'], '/save_loop_error')
    #     start_t = time.monotonic()
    #     while time.monotonic() - start_t < 2.0 and not os.path.isfile(wav_file):
    #         time.sleep(0.1)
    #     if not os.path.isfile(wav_file):
    #         raise Exception("Failed to save wav file for loop.")
    
    @pyqtSlot(result=str)
    def looper_type(self):
        return "BackendLooperManager"
