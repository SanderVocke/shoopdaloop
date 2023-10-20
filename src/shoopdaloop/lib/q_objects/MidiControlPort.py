from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQuick import QQuickItem

from ..logging import Logger as BaseLogger
from ..findFirstParent import findFirstParent

from .AutoConnect import AutoConnect
from ..backend_wrappers import *
from ..midi_helpers import *

from ..lua_qobject_interface import create_lua_qobject_interface, lua_int
class MidiControlPort(QQuickItem):
    
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
        
        # Track CC states
        self._cc_states = [[ None for cc in range(128)] for channel in range(128)]
        # Track active notes (set of (chan, note))
        self._active_notes = set()
        
        self._autoconnecters = []
        self.nameChanged.connect(self.autoconnect_update)
        self.autoconnect_regexesChanged.connect(self.autoconnect_update)
        self.directionChanged.connect(self.autoconnect_update)
    
        self._lua_obj = None
        
        self.rescan_parents()
        if not self._backend:
            self.parentChanged.connect(self.rescan_parents)
    
    ######################
    ## SIGNALS
    ######################
    msgReceived = Signal(list)

    ######################
    # PROPERTIES
    ######################
    
    # lua_interface
    luaInterfaceChanged = Signal('QVariant')
    @Property('QVariant', notify=luaInterfaceChanged)
    def lua_interface(self):
        return self._lua_obj
    
    # autoconnect_regexes
    autoconnect_regexesChanged = Signal(list)
    @Property(list, notify=autoconnect_regexesChanged)
    def autoconnect_regexes(self):
        return self._autoconnect_regexes
    @autoconnect_regexes.setter
    def autoconnect_regexes(self, l):
        if l != self._autoconnect_regexes:
            self._autoconnect_regexes = l
            self.autoconnect_regexesChanged.emit(l)

    # name_hint
    nameHintChanged = Signal(str)
    @Property(str, notify=nameHintChanged)
    def name_hint(self):
        return self._name_hint if self._name_hint else ''
    @name_hint.setter
    def name_hint(self, l):
        if l and l != self._name_hint:
            self._name_hint = l
            self.nameHintChanged.emit(l)
            self.maybe_init()
    
    # name
    nameChanged = Signal(str)
    @Property(str, notify=nameChanged)
    def name(self):
        return self._name
    
    # direction
    directionChanged = Signal(int)
    @Property(int, notify=directionChanged)
    def direction(self):
        return self._direction if self._direction else 0
    @direction.setter
    def direction(self, l):
        if l != self._direction:
            self._direction = l
            self.directionChanged.emit(l)
            self.maybe_init()
    
    # initialized
    initializedChanged = Signal(bool)
    @Property(bool, notify=initializedChanged)
    def initialized(self):
        return self._backend_obj != None
    
    ###########
    ## METHODS
    ###########
    
    def handle_msg(self, msg):
        if is_noteOn(msg):
            self._active_notes.add((channel(msg), note(msg)))
        elif is_noteOff(msg):
            self._active_notes.discard((channel(msg), note(msg)))
        elif is_cc(msg):
            self._cc_states[channel(msg)][msg['data'][1]] = msg['data'][2]
        self.msgReceived.emit(msg)
    
    ###########
    ## SLOTS
    ###########
    
    @Slot('QVariant')
    def register_lua_interface(self, scripting_engine):
        # Create a Lua interface for ourselves
        self._lua_obj = create_lua_qobject_interface(scripting_engine, self)
    
    @Slot(int, int, result='QVariant')
    def get_cc_state(self, channel, cc):
        """
        @shoop_lua_fn_docstring.start
        midi_control_port.get_cc_state(channel : int, cc : int) -> int / nil
        Get the current state (as known since port opened) of the given CC.
        @shoop_lua_fn_docstring.end
        """
        return self._cc_states[channel][cc]
    
    @Slot(result=list)
    def get_active_notes(self):
        """
        @shoop_lua_fn_docstring.start
        midi_control_port.get_active_notes() -> list of [int, int]
        Get the current set of active ("on") notes ([channel, note]), tracked since port opened.
        @shoop_lua_fn_docstring.end
        """
        return [list(_tuple) for _tuple in self._active_notes]
    
    @Slot()
    def autoconnect_update(self):
        if self._name and self._autoconnect_regexes and self._direction != None:
            for conn in self._autoconnecters:
                conn.destroy()
            self._autoconnecters = []
            
            if self._direction == PortDirection.Input.value:
                for regex in self._autoconnect_regexes:
                    conn = AutoConnect(self)
                    conn.from_regex = regex
                    conn.to_regex = self._name.replace('.', '\.')
                    self._autoconnecters.append(conn)
            else:
                for regex in self._autoconnect_regexes:
                    conn = AutoConnect(self)
                    conn.from_regex = self._name.replace('.', '\.')
                    conn.to_regex = regex
                    self._autoconnecters.append(conn)
    
    @Slot()
    def poll(self):
        while True:
            r = self._backend_obj.maybe_next_message()
            if not r:
                break
            self.logger.trace(lambda: "Received: {}".format(r.data))
            self.handle_msg(r.data)
    
    @Slot()
    def rescan_parents(self):
        maybe_backend = findFirstParent(self, lambda p: p and isinstance(p, QQuickItem) and p.inherits('Backend') and self._backend == None)
        if maybe_backend:
            self._backend = maybe_backend
            self.maybe_init()
    
    @Slot()
    def maybe_init(self):
        if self._backend and not self._backend.get_backend_obj():
            self._backend.initializedChanged.connect(self.maybe_init)
            return
        if self._name_hint and self._backend and self._direction != None:
            if self._backend_obj:
                return
            
            self.logger.debug(lambda: "Opening decoupled MIDI port {}".format(self._name_hint))
            self._backend_obj = self._backend.get_backend_obj().open_decoupled_midi_port(self._name_hint, self._direction)
            
            if not self._backend_obj:
                self.logger.error(lambda: "Failed to open decoupled MIDI port {}".format(self._name_hint))
                return

            self._name = self._backend_obj.name()
            self.nameChanged.emit(self._name)
            
            self.timer = QTimer(self)
            self.timer.setSingleShot(False)
            self.timer.setInterval(0)
            self.timer.timeout.connect(self.poll)
            self.timer.start()
            
            self.initializedChanged.emit(True)
    