from .ControlHandler import *
from ..logging import Logger

import inspect

from PySide6.QtCore import QObject, Signal, Property, Slot, Qt
from enum import Enum

from ..types import *

class ControlInterface(ControlHandler):
    """
    Extends ControlHandler with additional functionality for interactive
    control of the application.
    For example, adds a mechanism to register callbacks for application events.
    """
    
    def generate_keyboard_constants():
        # @shoop_lua_enum_docstring.start
        # shoop_control.constants.Key_
        # Keyboard key identifiers. Equal to Qt.Key_* (see Qt documentation).
        # ...
        # @shoop_lua_enum_docstring.end
        rval = []
        for i in list(Qt.Key):
            rval.append([i.name, i.value])
        for i in list(Qt.KeyboardModifier):
            rval.append([i.name, i.value])
        return rval
    
    lua_interfaces = ControlHandler.lua_interfaces + [
        ['auto_open_device_specific_midi_control_input', lua_str, lua_callable ],
        ['auto_open_device_specific_midi_control_output', lua_str, lua_callable, lua_callable ],
        ['register_keyboard_event_cb', lua_callable ],
        ['register_loop_event_cb', lua_callable ]
    ]
    
    # @shoop_lua_enum_docstring.start
    # shoop_control.constants.KeyEventType_
    # Keyboard event type identifier.
    # Pressed
    # Released
    # @shoop_lua_enum_docstring.end
    lua_constants = ControlHandler.lua_constants + [
        [ 'KeyEventType_Pressed', KeyEventType.Pressed ],
        [ 'KeyEventType_Released', KeyEventType.Released ],
    ] + generate_keyboard_constants()
    
    def __init__(self, parent=None):
        super(ControlInterface, self).__init__(parent)
        self._keyboard_callbacks = []
        self._midi_input_port_rules = []
        self._midi_output_port_rules = []
        self._loop_callbacks = []
        self._rule_id = 0
        self.logger = Logger('Frontend.ControlInterface')
    
    # Functions not meant for Lua use
    
    @Slot(int, int)
    def key_pressed(self, key, modifiers):
        for cb in self._keyboard_callbacks:
            cb(KeyEventType.Pressed, key, modifiers)
    
    @Slot(int, int)
    def key_released(self, key, modifiers):
        for cb in self._keyboard_callbacks:
            cb(KeyEventType.Released, key, modifiers)
    
    @Slot(list, 'QVariant', 'QVariant')
    def loop_event(self, coords, event, scripting_engine):
        if isinstance(event, QJSValue):
            event = event.toVariant()
        if isinstance(coords, QJSValue):
            coords = coords.toVariant()
        if scripting_engine:
            coords = scripting_engine.to_lua_val(coords)
            event = scripting_engine.to_lua_val(event)
        for cb in self._loop_callbacks:
            cb(coords, event)
            
    midiInputPortRulesChanged = Signal()
    @Property('QVariant', notify=midiInputPortRulesChanged)
    def midi_input_port_rules(self):
        return self._midi_input_port_rules
    
    midiOutputPortRulesChanged = Signal()
    @Property('QVariant', notify=midiOutputPortRulesChanged)
    def midi_output_port_rules(self):
        return self._midi_output_port_rules
    
    # Functions meant for Lua use
            
    @Slot(str, 'QVariant')
    def auto_open_device_specific_midi_control_input(self, device_name_filter_regex, msg_cb):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.auto_open_device_specific_midi_control_input(device_name_filter_regex, message_callback)
        Instruct the application to automatically open a MIDI control input port if a device matching the regex appears, and connect to it.
        Also registers a callback for received MIDI events on such a port. See midi_callback for details.
        @shoop_lua_fn_docstring.end
        """
        self.logger.debug(lambda: "Registering MIDI input control port rule for devices '{}'".format(device_name_filter_regex))
        self._midi_input_port_rules.append({
            'id': self._rule_id,
            'regex': device_name_filter_regex,
            'msg_cb': msg_cb
        })
        self.midiInputPortRulesChanged.emit()
        self._rule_id += 1
    
    @Slot(str, 'QVariant', 'QVariant')
    def auto_open_device_specific_midi_control_output(self, device_name_filter_regex, opened_cb, connected_cb):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.auto_open_device_specific_midi_control_output(device_name_filter_regex, opened_callback)
        Instruct the application to automatically open a MIDI control output port if a device matching the regex appears, and connect to it.
        Also registers a callback for when the port is opened and connected. This callback just passes a port object which has a 'send' method to send bytes.
        @shoop_lua_fn_docstring.end
        """
        self.logger.debug(lambda: "Registering MIDI output control port rule for devices '{}'".format(device_name_filter_regex))
        self._midi_output_port_rules.append({
            'id': self._rule_id,
            'regex': device_name_filter_regex,
            'opened_cb': opened_cb,
            'connected_cb': connected_cb
        })
        self.midiOutputPortRulesChanged.emit()
        self._rule_id += 1
    
    @Slot('QVariant')
    def register_keyboard_event_cb(self, cb):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.register_keyboard_event_cb(callback)
        Register a callback for keyboard events. See keyboard_callback for details.
        @shoop_lua_fn_docstring.end
        """
        self.logger.debug(lambda: "Registering keyboard event callback")
        self._keyboard_callbacks.append(cb)
        
    # @shoop_lua_fn_docstring.start
    # midi_callback(message : midi_message, port : midi_control_port)
    # Callback type for MIDI control message received. See midi_message and midi_control_port types for details.
    # @shoop_lua_fn_docstring.end
    
    # @shoop_lua_fn_docstring.start
    # keyboard_callback(event_type : shoop.constants.KeyEventType_[Pressed, Released], key : shoop.constants.Key_[...], modifiers : shoop.constants.[Shift,Control,Alt]Modifier)
    # Callback type for keyboard events.
    # @shoop_lua_fn_docstring.end
    
    # @shoop_lua_fn_docstring.start
    # type midi_message
    # MIDI message type. Fields are: bytes (array of message bytes), note, channel, cc, value, program, velocity (only those fields which apply to the particular message).
    # @shoop_lua_fn_docstring.end

    @Slot('QVariant')
    def register_loop_event_cb(self, cb):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.register_loop_event_cb(callback)
        Register a callback for loop events. See loop_callback for details.
        @shoop_lua_fn_docstring.end
        """
        self.logger.debug(lambda: "Registering loop event callback")
        self._loop_callbacks.append(cb)