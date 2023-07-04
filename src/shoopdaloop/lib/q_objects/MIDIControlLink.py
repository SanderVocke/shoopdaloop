import threading
import queue
from ctypes import *
from dataclasses import dataclass
from typing import *
from pprint import pprint
from threading import Thread
import traceback
import time

from PySide6.QtCore import QObject, Signal, Slot, QMetaObject, Qt

from .BackendManager import BackendManager, SlowMidiCallback

import sys
sys.path.append('../..')
from third_party.pyjacklib import jacklib
from third_party.pyjacklib.jacklib import helpers

# Implements a MIDI control link by instantiating a pair of Jack ports
# and providing signals and slots for MIDI communication over that
# specific pair.
class MIDIControlLink(QObject):
    received = Signal(list) # List of MIDI bytes in the message

    def __init__(self, parent, maybe_output_port_name, maybe_input_port_name, jack_client, backend_mgr):
        super(MIDIControlLink, self).__init__(parent)
        self._ready = False
        self._snd_queue = queue.Queue()
        self._jack_client = jack_client
        self._backend_mgr = backend_mgr
        self._input_port = self._output_port = None
        if maybe_input_port_name:
            self._rcv_cb = SlowMidiCallback(lambda data: self.received.emit(data))
            self._backend_input_port = backend_mgr.create_slow_midi_input(
               maybe_input_port_name, self._rcv_cb)
            if not self._backend_input_port:
                raise RuntimeError('Unable to create input port "{}" for MIDI control link'.format(maybe_input_port_name))
            self._jack_input_port = backend_mgr.convert_to_jack_port(self._backend_input_port)
        if maybe_output_port_name:
            self._backend_output_port = backend_mgr.create_slow_midi_output(maybe_output_port_name)
            if not self._backend_output_port:
                raise RuntimeError('Unable to create output port "{}" for MIDI control link'.format(maybe_output_port_name))
            self._jack_output_port = backend_mgr.convert_to_jack_port(self._backend_output_port)
        self._ready = True

    def is_input_connected_to(self, other_name):
            return jacklib.port_connected_to(self._jack_input_port, other_name)
    
    def is_output_connected_to(self, other_name):
            return jacklib.port_connected_to(self._jack_output_port, other_name)
    
    def connect_output(self, other_name):
            if self._jack_output_port == None:
                raise RuntimeError("Connect_output called for MIDI control link without output port")
            return jacklib.connect(self._jack_client, jacklib.port_name(self._jack_output_port), other_name)
    
    def connect_input(self, other_name):
            if self._jack_input_port == None:
                raise RuntimeError("Connect_input called for MIDI control link without input port: {}".format(self._jack_input_port))
            return jacklib.connect(self._jack_client, other_name, jacklib.port_name(self._jack_input_port))
    
    # List of MIDI bytes to send as a message
    @Slot(list)
    def send(self, msg):
        if self._backend_output_port:
            self._backend_mgr.send_slow_midi(self._backend_output_port, msg)
    
    @Slot()
    def destroy(self):
        self._ready = False
        if self._backend_input_port:
            self._backend_mgr.destroy_slow_midi_port(self._backend_input_port)
        if self._backend_output_port:
            self._backend_mgr.destroy_slow_midi_port(self._backend_output_port)

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
    link_created = Signal(AutoconnectRule, MIDIControlLink)
    link_destroyed = Signal(AutoconnectRule)

    def __init__(self, parent, jack_client, backend_mgr):
        super(MIDIControlLinkManager, self).__init__(parent)
        self._snd_queue = queue.Queue()
        self._jack_client = jack_client
        self._links : dict[Type[AutoconnectRule], Type[MIDIControlLink]] = {}
        self._rules : set[Type[AutoconnectRule]] = []
        self._lock = threading.RLock()
        self._backend_mgr = backend_mgr

        process_thread = Thread(target=lambda: self.process_func(), daemon=True)
        process_thread.start()
    
    def process_func(self):
        while True:
            self._backend_mgr.process_slow_midi()
            time.sleep(0.01)
    
    @Slot(list)
    def set_rules(self, rules : set[Type[AutoconnectRule]]):
        with self._lock:
            if rules != self._rules:
                self._rules = rules
                QMetaObject.invokeMethod(self, "update", Qt.ConnectionType.QueuedConnection)

    @Slot()
    def update(self):
        try:
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
                            self._backend_mgr
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
        except Exception as e:
            print("Exception during MIDI control handling:\n{}".format(str(e)))
            traceback.print_exc()

                
