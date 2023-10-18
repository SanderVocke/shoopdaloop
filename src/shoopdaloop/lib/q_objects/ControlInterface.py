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
        ['register_midi_event_cb', lua_str, lua_callable ],
        ['register_keyboard_event_cb', lua_callable ],
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
        self._midi_callbacks = []
        self.logger = Logger('Frontend.ControlInterface')
    
    @Slot(int, int)
    def key_pressed(self, key, modifiers):
        for cb in self._keyboard_callbacks:
            cb(KeyEventType.Pressed, key, modifiers)
    
    @Slot(int, int)
    def key_released(self, key, modifiers):
        for cb in self._keyboard_callbacks:
            cb(KeyEventType.Released, key, modifiers)
            
    @Slot(str, 'QVariant')
    def register_midi_event_cb(self, device_name_filter_regex, cb):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.register_midi_event_cb(device_name_filter_regex, callback)
        Register a callback for MIDI events. TODO: fill in further
        @shoop_lua_fn_docstring.end
        """
        self.logger.debug(lambda: "Registering MIDI event callback for device '{}'".format(device_name_filter_regex))
        self._midi_callbacks.append([device_name_filter_regex, cb])
    
    @Slot('QVariant')
    def register_keyboard_event_cb(self, cb):
        """
        @shoop_lua_fn_docstring.start
        shoop_control.register_keyboard_event_cb(callback)
        Register a callback for keyboard events.

        The callback should have the signature callback(event_type, key, modifiers), where:
        - event_type is a KeyEventType enum value (example in LUA: shoop.constants.KeyEventType_Pressed or shoop.constants.KeyEventType_Released)
        - key is the keycode as used by Qt.Key (example in LUA: shoop.constants.Key_A)
        - modifiers is a bitmask of Qt.KeyboardModifier values (example in LUA: shoop.constants.ShiftModifier)
        @shoop_lua_fn_docstring.end
        """
        self.logger.debug(lambda: "Registering keyboard event callback")
        self._keyboard_callbacks.append(cb)
    
    
    