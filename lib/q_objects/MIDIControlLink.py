import threading
import queue
from ctypes import *

from PyQt6.QtCore import QObject, pyqtSignal, pyqtSlot

import sys
sys.path.append('../..')
from third_party.pyjacklib import jacklib

class MIDIControlLink(QObject):
    # List is e.g. ['/some/path/stuff', 0, 1, 'textarg']. For any received message.
    received = pyqtSignal(list) # List of MIDI bytes in the message
    sent = pyqtSignal(list) # List of MIDI bytes in the message

    # TODO for loop parameter specifically

    def process_callback(self, nframes):
        # Input message processing
        inbuf = self._jack.port_get_buffer(self._input_port, nframes)
        if inbuf:
            event_cnt = self._jack.midi_get_event_count(inbuf)
            event = jacklib.jack_midi_event_t()
            for i in range(event_cnt):
                res = self._jack.midi_event_get(byref(event), inbuf, i)
                msg = [event.buffer[b] for b in range(event.size)]
                self.received.emit(msg)
        
        # Output message processing
        outbuf = self._jack.port_get_buffer(self._output_port, nframes)
        if outbuf:
            self._jack.midi_clear_buffer(outbuf)
            while not self._snd_queue.empty():
                msg = self._snd_queue.get(block=False)
                msg_bytes = (c_ubyte * len(msg))()
                for idx in range(len(msg)):
                    msg_bytes[idx] = c_ubyte(msg[idx])
                self._jack.midi_event_write(outbuf, 0, msg_bytes, len(msg))

        return 0

    def __init__(self, parent, output_port_name, input_port_name, jack_client, jack_lib_instance):
        super(MIDIControlLink, self).__init__(parent)
        self._snd_queue = queue.Queue()
        self._jack_client = jack_client
        self._jack = jack_lib_instance

        self._input_port = self._jack.port_register(self._jack_client, 'control_in', jacklib.JACK_DEFAULT_MIDI_TYPE, jacklib.JackPortIsInput, 0)
        self._output_port = self._jack.port_register(self._jack_client, 'control_out', jacklib.JACK_DEFAULT_MIDI_TYPE, jacklib.JackPortIsOutput, 0)
        self._process_callback = CFUNCTYPE(c_int, c_uint32, c_void_p)(lambda nframes, arg: self.process_callback(nframes))
        if self._jack.set_process_callback(self._jack_client, self._process_callback, None) != 0:
            raise Exception('Unable to set JACK process callback for MIDI control')
        
        self._jack.activate(self._jack_client)
    
    # List of MIDI bytes to send as a message
    @pyqtSlot(list)
    def send(self, msg):
        self._snd_queue.put(msg)
