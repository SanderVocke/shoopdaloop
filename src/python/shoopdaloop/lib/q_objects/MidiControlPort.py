from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer, QMetaType, QMetaObject, SIGNAL, SLOT
from PySide6.QtQuick import QQuickItem

import json

from .ShoopPyObject import *

from .Logger import Logger as BaseLogger

from ..backend_wrappers import *
from ..midi_helpers import *
from shoop_rust import shoop_rust_create_autoconnect
import shoop_py_backend

from shiboken6 import Shiboken

from ..lua_qobject_interface import create_lua_qobject_interface, lua_int
class MidiControlPort(ShoopQQuickItem):

    # MIDI control port has several Lua interfaces to query its state
    # from the Lua side.
    lua_interfaces: [
        [ 'get_cc_state', lua_int, lua_int ],
        [ 'get_active_notes' ]
    ]

    def __init__(self, parent=None):
        super(MidiControlPort, self).__init__(parent)
        self._name_hint = None
        self._direction = None
        self._backend_obj = None
        self.logger = BaseLogger(parent=self)
        self.logger.name = "Frontend.MidiControlPort"
        self._backend = None
        self._autoconnect_regexes = []
        self._name = None
        self._may_open = False
        self._initialized = False
        self._send_rate_limit_hz = 0
        self._send_queue = []

        self._send_timer = QTimer(self)
        self._send_timer.setSingleShot(True)
        self._send_timer.timeout.connect(self.update_send_queue)
        self._send_timer.setInterval(1)

        # Track CC states
        self._cc_states = [[ None for cc in range(128)] for channel in range(128)]
        # Track active notes (set of (chan, note))
        self._active_notes = set()

        self._autoconnecters = []
        self.nameChanged.connect(lambda: QTimer.singleShot(1, lambda: self.autoconnect_update()))
        self.autoconnect_regexesChanged.connect(lambda: QTimer.singleShot(1, lambda: self.autoconnect_update()))
        self.directionChanged.connect(lambda: QTimer.singleShot(1, lambda: self.autoconnect_update()))
        self.autoconnect_update()

        self._lua_obj = None

        def on_backend_changed(backend):
            self.logger.debug(lambda: 'Backend changed')
            QObject.connect(backend, SIGNAL("readyChanged()"), self, SLOT("maybe_init()"))
            self.maybe_init()
        self.backendChanged.connect(lambda b: on_backend_changed(b))

        self.msgReceived.connect(lambda msg: self.logger.debug(lambda: "Received: {}".format(msg)))
        self.connected.connect(lambda: self.logger.debug(lambda: "{}: connected".format(self._name_hint)))

    ######################
    ## SIGNALS
    ######################
    msgReceived = ShoopSignal(list)
    detectedExternalAutoconnectPartnerWhileClosed = ShoopSignal()
    connected = ShoopSignal()

    ######################
    # PROPERTIES
    ######################
    
    # backend
    backendChanged = ShoopSignal('QVariant')
    @ShoopProperty('QVariant', notify=backendChanged)
    def backend(self):
        return self._backend
    @backend.setter
    def backend(self, l):
        if l and l != self._backend:
            if self._backend or self._backend_obj:
                self.logger.throw_error('May not change backend of existing loop')
            self._backend = l
            self.logger.trace(lambda: 'Set backend -> {}'.format(l))
            self.backendChanged.emit(l)
            self.maybe_init()

    # initialized
    initializedChanged = ShoopSignal(bool)
    @ShoopProperty(bool, notify=initializedChanged)
    def initialized(self):
        return self._backend_obj != None

    # lua_interface
    luaInterfaceChanged = ShoopSignal('QVariant')
    @ShoopProperty('QVariant', notify=luaInterfaceChanged)
    def lua_interface(self):
        return self._lua_obj

    # autoconnect_regexes
    autoconnect_regexesChanged = ShoopSignal(list)
    @ShoopProperty(list, notify=autoconnect_regexesChanged)
    def autoconnect_regexes(self):
        return self._autoconnect_regexes
    @autoconnect_regexes.setter
    def autoconnect_regexes(self, l):
        if l != self._autoconnect_regexes:
            self.logger.trace(lambda: f'autoconnect_regexes -> {l}')
            self._autoconnect_regexes = l
            self.autoconnect_regexesChanged.emit(l)

    # name_hint
    nameHintChanged = ShoopSignal(str)
    @ShoopProperty(str, notify=nameHintChanged)
    def name_hint(self):
        return self._name_hint if self._name_hint else ''
    @name_hint.setter
    def name_hint(self, l):
        if l and l != self._name_hint:
            self._name_hint = l
            self.logger.trace(lambda: f'name_hint -> {l}')
            self.nameHintChanged.emit(l)
            self.maybe_init()

    # name
    nameChanged = ShoopSignal(str)
    @ShoopProperty(str, notify=nameChanged)
    def name(self):
        return self._name

    # direction
    directionChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=directionChanged)
    def direction(self):
        return self._direction if self._direction else 0
    @direction.setter
    def direction(self, l):
        if l != self._direction:
            self._direction = l
            self.logger.trace(lambda: f'direction -> {l}')
            self.directionChanged.emit(l)
            self.maybe_init()

    # dataType
    @ShoopProperty(int)
    def data_type(self):
        return int(shoop_py_backend.PortDataType.Midi)

    # may_open
    mayOpenChanged = ShoopSignal(bool)
    @ShoopProperty(bool, notify=mayOpenChanged)
    def may_open(self):
        return self._may_open
    @may_open.setter
    def may_open(self, l):
        if l != self._may_open:
            self._may_open = l
            self.mayOpenChanged.emit(l)
            self.maybe_init()

    # send_rate_limit_hz
    sendRateLimitHzChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=sendRateLimitHzChanged)
    def send_rate_limit_hz(self):
        return self._send_rate_limit_hz
    @send_rate_limit_hz.setter
    def send_rate_limit_hz(self, l):
        if l != self._send_rate_limit_hz:
            self._send_rate_limit_hz = l
            self.logger.trace(lambda: f'send rate limit -> {l}')
            self._send_timer.setInterval(1000/l)
            self.sendRateLimitHzChanged.emit(l)

    ###########
    ## METHODS
    ###########

    def handle_msg(self, msg):
        if is_noteOn(msg):
            self._active_notes.add((channel(msg), note(msg)))
        elif is_noteOff(msg):
            self._active_notes.discard((channel(msg), note(msg)))
        elif is_cc(msg):
            self._cc_states[channel(msg)][msg[1]] = msg[2]
        self.msgReceived.emit(msg)

    def get_data_type(self):
        return int(shoop_py_backend.PortDataType.Midi)

    ###########
    ## SLOTS
    ###########

    @ShoopSlot(result='QVariant')
    def get_connections_state(self):
        if self._backend_obj:
            self.logger.trace(lambda: "Getting connections state")
            return self._backend_obj.get_connections_state()
        else:
            self.logger.trace(lambda: "Attempted to get connections state of uninitialized port {}".format(self._name_hint))
            return dict()

    @ShoopSlot(result="QMap<QString,QVariant>")
    def determine_connections_state(self):
        rval = self.get_connections_state()
        self.logger.debug(lambda: f'connections state: {json.dumps(rval)}')
        return rval

    @ShoopSlot(str)
    def connect_external_port(self, name):
        if self._backend_obj:
            self.logger.debug(lambda: "Connecting to external port {}".format(name))
            self._backend_obj.connect_external_port(name)
        else:
            self.logger.error(lambda: "Attempted to connect uninitialized port {}".format(self._name_hint))

    @ShoopSlot(str)
    def disconnect_external_port(self, name):
        if self._backend_obj:
            self.logger.debug(lambda: "Disconnecting from external port {}".format(name))
            self._backend_obj.disconnect_external_port(name)
        else:
            self.logger.error(lambda: "Attempted to disconnect uninitialized port {}".format(self._name_hint))

    @ShoopSlot('QVariant')
    def register_lua_interface(self, lua_engine):
        # Create a Lua interface for ourselves
        self._lua_obj = create_lua_qobject_interface(lua_engine, self)

    @ShoopSlot(int, int, result='QVariant')
    def get_cc_state(self, channel, cc):
        """
        @shoop_lua_fn_docstring.start
        midi_control_port.get_cc_state(channel : int, cc : int) -> int / nil
        Get the current state (as known since port opened) of the given CC.
        @shoop_lua_fn_docstring.end
        """
        return self._cc_states[channel][cc]

    @ShoopSlot(result=list)
    def get_active_notes(self):
        """
        @shoop_lua_fn_docstring.start
        midi_control_port.get_active_notes() -> list of [int, int]
        Get the current set of active ("on") notes ([channel, note]), tracked since port opened.
        @shoop_lua_fn_docstring.end
        """
        return [list(_tuple) for _tuple in self._active_notes]

    @ShoopSlot()
    def update_send_queue(self):
        def send(msg):
            self.logger.debug(lambda: "Sending: {}".format(msg))
            if self._direction == int(shoop_py_backend.PortDirection.Output) and self._backend_obj:
                self._backend_obj.send_midi(bytes(msg))

        if self._send_rate_limit_hz == 0:
            while len(self._send_queue) > 0:
                send(self._send_queue.pop(0))
        elif len(self._send_queue) > 0:
            msg = self._send_queue.pop(0)
            send(msg)
            if len(self._send_queue) > 0:
                self._send_timer.start()

    @ShoopSlot(list)
    def send_msg(self, msg):
        # NOTE: not for direct use from Lua.
        # Sends the given bytes as a MIDI message.
        self._send_queue.append(msg)
        if self._send_rate_limit_hz > 0:
            self._send_timer.start()
        else:
            self.update_send_queue()

    @ShoopSlot()
    def autoconnect_update(self):
        global shoop_rust_create_autoconnect
        self.logger.trace(lambda: f"autoconnect_update {self._autoconnect_regexes} {self._direction} {self._autoconnecters}")
        if self._autoconnect_regexes and self._direction != None:
            for conn in self._autoconnecters:
                meta = conn.metaObject()
                method_idx = meta.indexOfMethod('destroy')
                method = meta.method(method_idx)
                method.invoke(conn)
                conn = None
            self._autoconnecters = []

            for regex in self._autoconnect_regexes:
                if not shoop_rust_create_autoconnect:
                    raise Exception("No construction function available for AutoConnect")
                inst = Shiboken.wrapInstance(shoop_rust_create_autoconnect(), QObject)
                if not isinstance(inst, QObject):
                    raise Exception("Created AutoConnect is not a QObject")
                inst.setParent(self)

                def set_property(auto_property_name, value):
                    auto_obj = inst.metaObject()
                    auto_prop_idx = auto_obj.indexOfProperty(auto_property_name)
                    auto_prop = auto_obj.property(auto_prop_idx)
                    if not auto_prop or not auto_prop.isValid():
                        raise Exception(f"Could not find property {auto_property_name}")
                    auto_prop.write(inst, value)

                set_property("parent", self)
                set_property("connect_to_port_regex", regex)
                set_property("internal_port", self)

                inst.connect(SIGNAL("only_external_found()"), lambda s=self: s.detectedExternalAutoconnectPartnerWhileClosed.emit())
                inst.connect(SIGNAL("connected()"), lambda s=self: s.connected.emit())

                self._autoconnecters.append(inst)

            self.logger.trace(lambda: f"# Autoconnecters: {len(self._autoconnecters)}")

            for autoconn in self._autoconnecters:
                meta = autoconn.metaObject()
                method_idx = meta.indexOfMethod('update')
                method = meta.method(method_idx)
                method.invoke(autoconn)

            self.logger.trace(lambda: f"Autoconnecters updated")

    @ShoopSlot()
    def close(self):
        self.logger.trace(lambda: "Closing.")
        self._backend_obj = None
        self._autoconnect_regexes = []
        self.autoconnect_update()

    @ShoopSlot()
    def poll(self):
        while True:
            r = self._backend_obj.maybe_next_message()
            if not r:
                break
            self.logger.debug(lambda: "Received: {}".format(r.data))
            self.handle_msg(r.data)

    @ShoopSlot()
    def maybe_init(self):
        if self._backend_obj:
            return
        self.logger.trace(lambda: f'Attempting to initialize. Backend: {self._backend}. Backend init: {self._backend.property("ready") if self._backend else "None"}')
        if self._backend and not self._backend.property('ready'):
            QObject.connect(self._backend, SIGNAL("readyChanged()"), self, SLOT("maybe_init()"))
            return
        if self._name_hint and self._backend and self._direction != None and self._may_open:
            self.logger.debug(lambda: "Opening decoupled MIDI port {}".format(self._name_hint))
            from shoop_rust import shoop_rust_open_driver_decoupled_midi_port
            from shiboken6 import getCppPointer
            self._backend_obj = shoop_rust_open_driver_decoupled_midi_port(
                    getCppPointer(self._backend)[0],
                    self._name_hint,
                    self._direction
                )

            if not self._backend_obj:
                self.logger.error(lambda: "Failed to open decoupled MIDI port {}".format(self._name_hint))
                return

            self._name = self._backend_obj.name()
            self.nameChanged.emit(self._name)

            if self._direction == int(shoop_py_backend.PortDirection.Input):
                self.timer = QTimer(self)
                self.timer.setSingleShot(False)
                self.timer.setInterval(10)
                self.timer.timeout.connect(self.poll)
                self.timer.start()

            self.initializedChanged.emit(True)

    @ShoopSlot(result='QVariant')
    def get_py_send_fn(self):
        def send_fn(msg, self=self):
            _msg = list(msg.values())
            self.send_msg(_msg)
        return lambda msg, fn=send_fn: fn(msg)