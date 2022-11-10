import threading
import queue
from ctypes import *

from PyQt6.QtCore import QObject, pyqtSignal, pyqtSlot, QMetaObject, Qt
from dataclasses import dataclass
from typing import *
from pprint import pprint

import sys
sys.path.append('../..')
from third_party.pyjacklib import jacklib
from third_party.pyjacklib.jacklib import helpers

# Implements a MIDI control link by instantiating a pair of Jack ports
# and providing signals and slots for MIDI communication over that
# specific pair.
class MIDIControlLink(QObject):
    received = pyqtSignal(list) # List of MIDI bytes in the message

    def __init__(self, parent, maybe_output_port_name, maybe_input_port_name, jack_client):
        super(MIDIControlLink, self).__init__(parent)
        self._ready = False
        self._snd_queue = queue.Queue()
        self._jack_client = jack_client
        self._input_port = self._output_port = None
        if maybe_input_port_name:
            self._input_port = jacklib.port_register(self._jack_client, maybe_input_port_name, jacklib.JACK_DEFAULT_MIDI_TYPE, jacklib.JackPortIsInput, 0)
        if maybe_output_port_name:
            self._output_port = jacklib.port_register(self._jack_client, maybe_output_port_name, jacklib.JACK_DEFAULT_MIDI_TYPE, jacklib.JackPortIsOutput, 0)
        self._ready = True

    def process(self, nframes):
        if not self._ready:
            return

        # Input message processing
        if self._input_port:
            inbuf = jacklib.port_get_buffer(self._input_port, nframes)
            if inbuf:
                event_cnt = jacklib.midi_get_event_count(inbuf)
                event = jacklib.jack_midi_event_t()
                for i in range(event_cnt):
                    res = jacklib.midi_event_get(byref(event), inbuf, i)
                    msg = [event.buffer[b] for b in range(event.size)]
                    self.received.emit(msg)
        
        # Output message processing
        if self._output_port:
            outbuf = jacklib.port_get_buffer(self._output_port, nframes)
            if outbuf:
                jacklib.midi_clear_buffer(outbuf)
                while not self._snd_queue.empty():
                    msg = self._snd_queue.get(block=False)
                    msg_bytes = (c_ubyte * len(msg))()
                    for idx in range(len(msg)):
                        msg_bytes[idx] = c_ubyte(msg[idx])
                    jacklib.midi_event_write(outbuf, 0, msg_bytes, len(msg))

    def is_input_connected_to(self, other_name):
            return jacklib.port_connected_to(self._input_port, other_name)
    
    def is_output_connected_to(self, other_name):
            return jacklib.port_connected_to(self._output_port, other_name)
    
    def connect_output(self, other_name):
            return jacklib.connect(self._jack_client, jacklib.port_name(self._output_port), other_name)
    
    def connect_input(self, other_name):
            return jacklib.connect(self._jack_client, other_name, jacklib.port_name(self._input_port))
    
    # List of MIDI bytes to send as a message
    @pyqtSlot(list)
    def send(self, msg):
        self._snd_queue.put(msg)
    
    @pyqtSlot()
    def destroy(self):
        if self._input_port:
            jacklib.port_unregister(self._jack_client, self._input_port)
        if self._output_port:
            jacklib.port_unregister(self._jack_client, self._output_port)

@dataclass(eq=True, frozen=True)
class AutoconnectRule:
    name: str
    input_regex: Union[None, str]
    output_regex: Union[None, str]
    our_input_port: Union[None, str]
    our_output_port: Union[None, str]
    always_active: bool

class MIDIControlLinkManager(QObject):
    # TODO for loop parameter specifically
    link_created = pyqtSignal(AutoconnectRule, MIDIControlLink)
    link_destroyed = pyqtSignal(AutoconnectRule)

    def __init__(self, parent, jack_client):
        super(MIDIControlLinkManager, self).__init__(parent)
        self._snd_queue = queue.Queue()
        self._jack_client = jack_client
        self._links : dict[Type[AutoconnectRule], Type[MIDIControlLink]] = {}
        self._rules : set[Type[AutoconnectRule]] = []
        self._lock = threading.RLock()

        self._process_callback = CFUNCTYPE(c_int, c_uint32, c_void_p)(lambda nframes, arg: self.process_callback(nframes))
        if jacklib.set_process_callback(self._jack_client, self._process_callback, None) != 0:
            raise Exception('Unable to set JACK process callback for MIDI control')
        
        jacklib.activate(self._jack_client)
    
    def process_callback(self, nframes):
        with self._lock:
            for l in self._links.values():
                l.process(nframes)
        return 0
    
    @pyqtSlot(list)
    def set_rules(self, rules : set[Type[AutoconnectRule]]):
        with self._lock:
            if rules != self._rules:
                self._rules = rules
                QMetaObject.invokeMethod(self, "update", Qt.ConnectionType.QueuedConnection)

    @pyqtSlot()
    def update(self):
        for rule in self._rules:
            if rule.always_active:
                if rule not in self._links:
                    print("MIDI control: creating permanent ports for '{}'".format(rule.name))
                    link = MIDIControlLink(
                        None,
                        rule.our_output_port,
                        rule.our_input_port,
                        self._jack_client,
                        jacklib
                    )
                    with self._lock:
                        self._links[rule] = link
                    self.link_created.emit(rule, link)

            if rule.input_regex or rule.output_regex:
                matching_input_ports = matching_output_ports = []
                if rule.input_regex:
                    matching_input_ports = helpers.c_char_p_p_to_list(
                        jacklib.get_ports(self._jack_client, rule.input_regex, None, jacklib.JackPortIsOutput)
                    )
                if rule.output_regex:
                    matching_output_ports = helpers.c_char_p_p_to_list(
                        jacklib.get_ports(self._jack_client, rule.output_regex, None, jacklib.JackPortIsInput)
                    )
                
                if len(matching_input_ports) + len(matching_output_ports) > 0 and \
                    rule not in self._links:
                    print('MIDI control: detected port for on-demand link {}'.format(rule.name))
                    link = MIDIControlLink(
                        None,
                        rule.our_output_port,
                        rule.our_input_port,
                        self._jack_client,
                        jacklib
                    )
                    with self._lock:
                        self._links[rule] = link
                    self.link_created.emit(rule, link)
                
                link = None
                if rule in self._links:
                    link = self._links[rule]
                for other_portname in matching_input_ports:
                    if rule.input_regex and not link.is_input_connected_to(other_portname):
                        print("MIDI control: connecting '{}' input to '{}'".format(rule.name, other_portname))
                        link.connect_input(other_portname)
                for other_portname in matching_output_ports:
                    if rule.output_regex and not link.is_output_connected_to(other_portname):
                        print("MIDI control: connecting control output to '{}'".format(other_portname))
                        link.connect_output(other_portname)

                if len(matching_input_ports) + len(matching_output_ports) == 0 and \
                    not rule.always_active:
                    # no matching ports found, cleanup
                    if rule in self._links:
                        print("MIDI control: removing on-demand link '{}'.".format(rule.name))
                        self._links[rule].destroy()
                        with self._lock:
                            del self._links[rule]

            
