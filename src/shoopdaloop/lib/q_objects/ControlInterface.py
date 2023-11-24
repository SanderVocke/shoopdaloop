from .ControlHandler import *
from ..logging import Logger

import inspect

from .ShoopPyObject import *

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

    engineRegisteredCallback = Signal('QVariant')
    engineUnregisteredAll = Signal('QVariant')

    @Slot('QVariant', result=bool)
    def engine_registered(self, engine):
        return any([cb[1] == engine for cb in self._keyboard_callbacks]) or \
            any([cb['engine'] == engine for cb in self._midi_input_port_rules]) or \
            any([cb['engine'] == engine for cb in self._midi_output_port_rules]) or \
            any([cb[1] == engine for cb in self._loop_callbacks])
    
    @Slot(int, int)
    def key_pressed(self, key, modifiers):
        self.logger.trace(lambda: "Key pressed: {} ({})".format(key, modifiers))
        for cb in self._keyboard_callbacks:
            cb[0](KeyEventType.Pressed, key, modifiers)
    
    @Slot(int, int)
    def key_released(self, key, modifiers):
        self.logger.trace(lambda: "Key pressed: {} ({})".format(key, modifiers))
        for cb in self._keyboard_callbacks:
            cb[0](KeyEventType.Released, key, modifiers)
    
    @Slot(list, 'QVariant')
    def loop_event(self, coords, event):
        if isinstance(event, QJSValue):
            event = event.toVariant()
        if isinstance(coords, QJSValue):
            coords = coords.toVariant()
        for cb in self._loop_callbacks:
            cb[0](coords, event)
            
    midiInputPortRulesChanged = Signal()
    @Property('QVariant', notify=midiInputPortRulesChanged)
    def midi_input_port_rules(self):
        return self._midi_input_port_rules
    
    midiOutputPortRulesChanged = Signal()
    @Property('QVariant', notify=midiOutputPortRulesChanged)
    def midi_output_port_rules(self):
        return self._midi_output_port_rules
    
    # Functions meant for Lua use
    
    @Slot('QVariant')
    def unregister_lua_engine(self, engine):
        self.logger.debug(lambda: "Unregistering Lua engine")
        self._keyboard_callbacks = [cb for cb in self._keyboard_callbacks if cb[1] != engine]
        self._midi_input_port_rules = [cb for cb in self._midi_input_port_rules if cb['engine'] != engine]
        self._midi_output_port_rules = [cb for cb in self._midi_output_port_rules if cb['engine'] != engine]
        self._loop_callbacks = [cb for cb in self._loop_callbacks if cb[1] != engine]
        self.midiInputPortRulesChanged.emit()
        self.midiOutputPortRulesChanged.emit()
        self.engineUnregisteredAll.emit(engine)
            
    @Slot(list, 'QVariant')
    def auto_open_device_specific_midi_control_input(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.auto_open_device_specific_midi_control_input(device_name_filter_regex, message_callback)
        Instruct the application to automatically open a MIDI control input port if a device matching the regex appears, and connect to it.
        Also registers a callback for received MIDI events on such a port. See midi_callback for details.
        @shoop_lua_fn_docstring.end
        """
        device_name_filter_regex = args[0]
        msg_cb = args[1]
        self.logger.debug(lambda: "Registering MIDI input control port rule for devices '{}'".format(device_name_filter_regex))
        self._midi_input_port_rules.append({
            'id': self._rule_id,
            'regex': device_name_filter_regex,
            'msg_cb': msg_cb,
            'engine': lua_engine
        })
        self.midiInputPortRulesChanged.emit()
        self._rule_id += 1
        self.engineRegisteredCallback.emit(lua_engine)
    
    @Slot(list, 'QVariant')
    def auto_open_device_specific_midi_control_output(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.auto_open_device_specific_midi_control_output(device_name_filter_regex, opened_callback, connected_callback)
        Instruct the application to automatically open a MIDI control output port if a device matching the regex appears, and connect to it.
        Also registers callbacks for when the port is opened and connected. This callbacks just pass a port object which has a 'send' method to send bytes.
        @shoop_lua_fn_docstring.end
        """
        device_name_filter_regex = args[0]
        opened_cb = args[1]
        connected_cb = args[2]
        self.logger.debug(lambda: "Registering MIDI output control port rule for devices '{}'".format(device_name_filter_regex))
        self._midi_output_port_rules.append({
            'id': self._rule_id,
            'regex': device_name_filter_regex,
            'opened_cb': opened_cb,
            'connected_cb': connected_cb,
            'engine': lua_engine
        })
        self.midiOutputPortRulesChanged.emit()
        self._rule_id += 1
        self.engineRegisteredCallback.emit(lua_engine)
    
    @Slot(list, 'QVariant')
    def register_keyboard_event_cb(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.register_keyboard_event_cb(callback)
        Register a callback for keyboard events. See keyboard_callback for details.
        @shoop_lua_fn_docstring.end
        """
        cb = args[0]
        self.logger.debug(lambda: "Registering keyboard event callback")
        self._keyboard_callbacks.append([cb, lua_engine])
        self.engineRegisteredCallback.emit(lua_engine)
        
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

    # @shoop_lua_fn_docstring.start
    # type loop_callback
    # Loop event callback type. The callback takes arguments (coords, event), where coords is [x, y] coordinates of the event, and event is a table containing fields 'mode' (mode), 'selected' (bool), 'targeted' (bool) and 'length' (int).
    # @shoop_lua_fn_docstring.end

    @Slot(list, 'QVariant')
    def register_loop_event_cb(self, args, lua_engine):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.register_loop_event_cb(callback)
        Register a callback for loop events. See loop_callback for details.
        @shoop_lua_fn_docstring.end
        """
        cb = lambda coords, event, _lua_engine=lua_engine, _cb=args[0]: _cb(_lua_engine.to_lua_val(coords), event)
        self.logger.debug(lambda: "Registering loop event callback")
        self._loop_callbacks.append([cb, lua_engine])
        self.engineRegisteredCallback.emit(lua_engine)