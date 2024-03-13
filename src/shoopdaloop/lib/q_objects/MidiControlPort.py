from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQuick import QQuickItem

from .ShoopPyObject import *
from .FindParentBackend import FindParentBackend

from ..logging import Logger as BaseLogger
from ..findFirstParent import findFirstParent

from .AutoConnect import AutoConnect

from ..backend_wrappers import *
from ..midi_helpers import *

from ..lua_qobject_interface import create_lua_qobject_interface, lua_int
class MidiControlPort(FindParentBackend):
    
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
        self.logger = BaseLogger("Frontend.MidiControlPort")
        self._backend = None
        self._autoconnect_regexes = []
        self._name = None
        self._may_open = False
        
        # Track CC states
        self._cc_states = [[ None for cc in range(128)] for channel in range(128)]
        # Track active notes (set of (chan, note))
        self._active_notes = set()
        
        self._autoconnecters = []
        self.nameChanged.connect(self.autoconnect_update)
        self.autoconnect_regexesChanged.connect(self.autoconnect_update)
        self.directionChanged.connect(self.autoconnect_update)
        self.autoconnect_update()
    
        self._lua_obj = None

        self.backendChanged.connect(lambda: self.maybe_init())
        self.backendInitializedChanged.connect(lambda: self.maybe_init())
        
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
            self.directionChanged.emit(l)
            self.maybe_init()
    
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
    
    ###########
    ## SLOTS
    ###########
    
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
    
    @ShoopSlot(list)
    def send_msg(self, msg):
        # NOTE: not for direct use from Lua.
        # Sends the given bytes as a MIDI message.
        self.logger.debug(lambda: "Sending: {}".format(msg))
        if self._direction == PortDirection.Output.value and self._backend_obj:
            self._backend_obj.send_midi(msg)
    
    @ShoopSlot()
    def autoconnect_update(self):
        if self._autoconnect_regexes and self._direction != None:
            for conn in self._autoconnecters:
                conn.destroy()
            self._autoconnecters = []
            
            if self._direction == PortDirection.Input.value:
                for regex in self._autoconnect_regexes:
                    conn = AutoConnect(self)
                    conn.connected.connect(self.connected)
                    conn.from_regex = regex
                    conn.to_regex = self._name.replace('.', '\.') if self._name else None
                    conn.onlyExternalFound.connect(self.detectedExternalAutoconnectPartnerWhileClosed)
                    self._autoconnecters.append(conn)
            else:
                for regex in self._autoconnect_regexes:
                    conn = AutoConnect(self)
                    conn.connected.connect(self.connected)
                    conn.from_regex = self._name.replace('.', '\.') if self._name else None
                    conn.to_regex = regex
                    conn.onlyExternalFound.connect(self.detectedExternalAutoconnectPartnerWhileClosed)
                    self._autoconnecters.append(conn)
    
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
        self.logger.trace(lambda: f'Attempting to initialize. Backend: {self._backend}. Backend init: {self._backend.initialized if self._backend else None}')
        if self._backend and not self._backend.initialized:
            self._backend.initializedChanged.connect(self.maybe_init)
            return
        if self._name_hint and self._backend and self._direction != None and self._may_open:
            if self._backend_obj:
                return
            
            self.logger.debug(lambda: "Opening decoupled MIDI port {}".format(self._name_hint))
            self._backend_obj = self._backend.get_backend_driver_obj().open_decoupled_midi_port(self._name_hint, self._direction)
            
            if not self._backend_obj:
                self.logger.error(lambda: "Failed to open decoupled MIDI port {}".format(self._name_hint))
                return

            self._name = self._backend_obj.name()
            self.nameChanged.emit(self._name)
            
            if self._direction == PortDirection.Input.value:
                self.timer = QTimer(self)
                self.timer.setSingleShot(False)
                self.timer.setInterval(0)
                self.timer.timeout.connect(self.poll)
                self.timer.start()
            
            self.backendInitializedChanged.emit(True)
    
    @ShoopSlot(result='QVariant')
    def get_py_send_fn(self):
        def send_fn(msg, self=self):
            _msg = list(msg.values())
            self.send_msg(_msg)
        return lambda msg, fn=send_fn: fn(msg)