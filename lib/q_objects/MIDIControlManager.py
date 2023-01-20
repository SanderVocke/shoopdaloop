from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer

from ..StatesAndActions import LoopState, MIDIMessageFilterType
from ..MidiScripting import *
from ..flatten import flatten
from .MIDIControlLink import *
from .BackendManager import BackendManager

from enum import Enum
from typing import *
from pprint import *
from copy import *
import time
from dataclasses import dataclass
import math

class InputRule:
    press_period = 0.5
    doublepress_period = 1.0

    def __init__(self,
                 filters : dict[Type[MIDIMessageFilterType], Union[None, int]],
                 condition_formulas : list[str],
                 formula : str,
                 ):
        self.filters = filters
        self.condition_formulas = condition_formulas
        self.formula = formula

        # internal
        self._lastOn = {}
        self._lastOff = {}
        self._lastPress = {}
    
    def apply_filters(self, msg_bytes):
        if len(msg_bytes) == 0:
            return False

        type_byte = msg_bytes[0] & 0xF0
        channel = msg_bytes[0] & 0x0F
        controller_or_note = None
        value_or_velocity = None
        filters = deepcopy(self.filters)
        press = False
        doublePress = False
        _time = time.monotonic()

        if len(msg_bytes) >= 2:
            controller_or_note = msg_bytes[1]
        
        if len(msg_bytes) >= 3:
            value_or_velocity = msg_bytes[2]
        
        if (MIDIMessageFilterType.IsNoteOff.value in filters or \
            MIDIMessageFilterType.IsNoteOn.value in filters or \
            MIDIMessageFilterType.IsNoteShortPress.value in filters or \
            MIDIMessageFilterType.IsNoteDoublePress.value in filters) and \
           MIDIMessageFilterType.IsNoteKind.value not in filters:
           filters[MIDIMessageFilterType.IsNoteKind.value] = None
        
        if type_byte == 0x80:
            if controller_or_note in self._lastOn and \
               (controller_or_note not in self._lastOff or self._lastOff[controller_or_note] < self._lastOn[controller_or_note]) and \
               _time - self._lastOn[controller_or_note] < self.press_period:
               press = True
            
            self._lastOff[controller_or_note] = _time

            if MIDIMessageFilterType.IsCCKind.value in filters or \
               MIDIMessageFilterType.IsNoteOn.value in filters:
                return False
               
        elif type_byte == 0x90:
            self._lastOn[controller_or_note] = _time
            if MIDIMessageFilterType.IsCCKind.value in filters or \
                MIDIMessageFilterType.IsNoteOff.value in filters:
                return False

        elif type_byte == 0xB0:
            if MIDIMessageFilterType.IsNoteKind.value in filters:
                return False
        
        if press:
            if controller_or_note in self._lastPress and \
               _time - self._lastPress[controller_or_note] < self.doublepress_period:
               doublePress = True
            self._lastPress[controller_or_note] = _time
        
        if MIDIMessageFilterType.IsNoteDoublePress.value in filters and not doublePress:
            return False
        
        if MIDIMessageFilterType.IsNoteShortPress.value in filters and not press:
            return False
        
        return True

def get_builtin_substitutions_for_message(msg_bytes):

    if len(msg_bytes) < 3:
        return {}
    
    type_byte = msg_bytes[0] & 0xF0
    channel = msg_bytes[0] & 0x0F

    if type_byte == 0x90:
        return {
            'type': 'noteOn',
            'note': msg_bytes[1],
            'velocity': msg_bytes[2],
            'channel': channel
        }
    elif type_byte == 0x80:
        return {
            'type': 'noteOff',
            'note': msg_bytes[1],
            'velocity': msg_bytes[2],
            'channel': channel
        }
    elif type_byte == 0xB0:
        return {
            'type': 'cc',
            'controller': msg_bytes[1],
            'value': msg_bytes[2],
            'channel': channel
        }
    
    return {}

# A MIDI control dialect describes how to communicate with a device
# using MIDI messages.
@dataclass
class MIDIControlDialect:
    # Names found in formula expressions to be substituted by subformulas.
    substitutions: dict[str, str]
    # All variables to track for the dialect need to be defined here with a default.
    variables: dict[str, Any]
    # Rules for handling MIDI input messages.
    input_rules: list[Type[InputRule]]
    # Formulas to be executed when loops change to a particular state.
    # After a connection of a controller, first the reset formula will be
    # executed and then the loop state formulas for all loops.
    loop_state_output_formulas: dict[Type[LoopState], str]
    # Formulas to be executed when loops change to a state not covered
    # by loop_state_output_formulas.
    loop_state_default_output_formula: Union[str, None]
    # Formula to be executed to reset a MIDI device (e.g. when connected,
    # when exiting the program)
    reset_output_formula: Union[str, None]

builtin_dialects = {
    'AKAI APC Mini': MIDIControlDialect(
        substitutions = {   # Substitutions to map buttons to loops and vice versa
            'loop_note':  '0 if (track == 0 and loop == 0) else (56-loop*8+track-1)',
            'note_track': 'note % 8 + 1',
            'note_loop':  '7 - (note // 8)',
            'fader_track': 'controller-48+1',
            'off': 0,
            'green': 1, 'blink_green': 2,
            'red': 3, 'blink_red': 4,
            'yellow': 5, 'blink_yellow': 6,
            'isSelectPressed': 'isNotePressed(0,86)',
            'isRecArmPressed': 'isNotePressed(0,84)'
        },
        variables = {},
        input_rules = [   # Rules
            InputRule({
                MIDIMessageFilterType.IsNoteOn.value: None,
            }, ['note <= 64'], 'loopAction(note_track, note_loop, "select" if isSelectPressed else ("record" if isRecArmPressed else "toggle_playing"), not isSelectPressed, 0)'),
            InputRule({
                MIDIMessageFilterType.IsNoteDoublePress.value: None,
            }, ['note <= 64 and isSelectPressed'], 'loopAction(note_track, note_loop, "target", False, 0)'),
            InputRule({
                MIDIMessageFilterType.IsCCKind.value: None,
            }, ['48 <= controller < 56'], 'setVolume(fader_track, value/127.0)')
        ],
        loop_state_output_formulas = {
            # Loop states to button colors
            LoopState.Recording.value: 'noteOn(0, loop_note, yellow if is_loop_targeted else (blink_red if is_loop_selected else red))',
            LoopState.Playing.value: 'noteOn(0, loop_note, yellow if is_loop_targeted else (blink_green if is_loop_selected else green))',
            LoopState.Stopped.value: 'noteOn(0, loop_note, yellow if is_loop_targeted else (blink_yellow if is_loop_selected else off))',
            LoopState.Unknown.value: 'noteOn(0, loop_note, yellow if is_loop_targeted else (blink_yellow if is_loop_selected else off))',
            LoopState.PlayingMuted.value: 'noteOn(0, loop_note, yellow if is_loop_targeted else (blink_green if is_loop_selected else green))',
            LoopState.PlayingLiveFX.value: 'noteOn(0, loop_note, yellow if is_loop_targeted else (blink_green if is_loop_selected else green))',
            LoopState.RecordingFX.value: 'noteOn(0, loop_note, yellow if is_loop_targeted else (blink_red if is_loop_selected else red))',
            LoopState.Empty.value: 'noteOn(0, loop_note, yellow if is_loop_targeted else (blink_yellow if is_loop_selected else off))',
        },
        # Any unmapped state maps to yellow
        loop_state_default_output_formula = 'noteOn(0, loop_note, 5)',
        # On reset, turn everything off
        reset_output_formula = 'notesOn(0, 0, 98, 0)',
        )
}

# A MIDI Controller object executes the MIDI communication
# for a given dialect.
class MIDIController(QObject):
    def __init__(self, dialect, control_manager, parent=None):
        super(MIDIController, self).__init__(parent)
        self._dialect = dialect
        self._manager = control_manager
        self._vars = dialect.variables
        self._notes_currently_on = set()
    
    sendMidi = pyqtSignal(list) # list of byte values
    loopAction = pyqtSignal(int, int, int, list) # track idx, loop idx, LoopActionType value, args
    setPan = pyqtSignal(int, float)
    setVolume = pyqtSignal(int, float)

    def set_var(self, name, value):
        if name not in self._vars.keys():
            raise Exception('MIDI dialect attempted to set var {}, which is not in its variables list.'.format(name))
        self._vars[name] = value
    
    def get_var(self, name):
        if name not in self._vars.keys():
            raise Exception('MIDI dialect attempted to get var {}, which is not in its variables list.'.format(name))
        return self._vars[name]

    def send_midi_messages(self, messages):
        for msg in messages:
            if isinstance(msg, MIDIMessage):
                self.sendMidi.emit(msg.bytes())
    
    def trigger_actions(self, actions):
        for action in actions:
            if isinstance(action, LoopAction):
                self.loopAction.emit(action.track_idx, action.loop_idx, action.action_type, action.args)
            elif isinstance(action, SetPan):
                self.setPan.emit(action.track_idx, action.value)
            elif isinstance(action, SetVolume):
                self.setVolume.emit(action.track_idx, action.value)
    
    @pyqtSlot(list)
    def receiveMidi(self, _bytes):
        if len(_bytes) < 1:
            return

        # Process notes on/off
        if _bytes[0] & 0xF0 == 0x80:
            self._notes_currently_on.remove((_bytes[0] & 0x0F, _bytes[1]))
        elif _bytes[0] & 0xF0 == 0x90:
            self._notes_currently_on.add((_bytes[0] & 0x0F, _bytes[1]))

        # Process formulas for msg        
        substitutions = {**get_builtin_substitutions_for_message(_bytes), **self._dialect.substitutions}
        formulas_to_execute = []
        for r in self._dialect.input_rules:
            if r.apply_filters(_bytes):
                execute = True
                for c in r.condition_formulas:
                    result = eval_formula(c,
                                          False,
                                          substitutions,
                                          self._notes_currently_on,
                                          {
                                            'get_var': lambda name: self.get_var(name),
                                            'set_var': lambda name, value: self.set_var(name, value)
                                          }
                                          )
                    if not result or not isinstance(result, bool):
                        execute = False
                if execute:
                    formulas_to_execute.append(r.formula)
        
        actions = flatten([eval_formula(
            f,
            True,
            substitutions,
            self._notes_currently_on,
            {
                'get_var': lambda name: self.get_var(name),
                'set_var': lambda name, value: self.set_var(name, value)
            }
            ) for f in formulas_to_execute])
        self.trigger_actions(actions)
    
    @pyqtSlot(int, int, int, bool, bool)
    def loop_state_changed(self, track, index, state, selected, targeted):
        substitutions = {
            'track': track,
            'loop': index,
            'state': state,
            'is_loop_selected': selected,
            'is_loop_targeted': targeted,
        }
        if state in self._dialect.loop_state_output_formulas:
            self.send_midi_messages(eval_formula(
                self._dialect.loop_state_output_formulas[state],
                True,
                {**substitutions, **self._dialect.substitutions},
                self._notes_currently_on,
                {
                    'get_var': lambda name: self.get_var(name),
                    'set_var': lambda name, value: self.set_var(name, value)
                }
                ))
        elif self._dialect.loop_state_default_output_formula:
            self.send_midi_messages(eval_formula(
                self._dialect.loop_state_default_output_formula,
                True,
                {**substitutions, **self._dialect.substitutions},
                self._notes_currently_on,
                {
                    'get_var': lambda name: self.get_var(name),
                    'set_var': lambda name, value: self.set_var(name, value)
                }
                ))

    @pyqtSlot(int)
    def active_sripting_section_changed(self, idx):
        raise NotImplementedError()
    
    @pyqtSlot(int)
    def active_scene_changed(self, idx):        
        raise NotImplementedError()
    
    @pyqtSlot()
    def reset(self):
        if self._dialect.reset_output_formula:
            self.send_midi_messages(eval_formula(
                self._dialect.reset_output_formula,
                True,
                self._dialect.substitutions,
                self._notes_currently_on,
                {
                    'get_var': lambda name: self.get_var(name),
                    'set_var': lambda name, value: self.set_var(name, value)
                }
                ))

# For communicating with MIDI control/input devices.
class MIDIControlManager(QObject):
    def __init__(self, parent, jack_client, backend_mgr):
        super(MIDIControlManager, self).__init__(parent)

        # TODO: hard-coded to the APC mini profile for now
        self._autoconnect_rules = {
            AutoconnectRule(
                'AKAI APC Mini',
                '.*APC MINI MIDI.*',
                '.*APC MINI MIDI.*',
                'APC Mini auto in',
                'APC Mini auto out',
                False
            ): builtin_dialects['AKAI APC Mini']
        }

        self._loop_states_cache = {} # tuple of (track, loop) => last transmitted state
        self._scripting_section_cache = None # last transmitted section
        self._active_scene_cache = None # last transmitted active scene

        self._link_manager = MIDIControlLinkManager(
            None,
            jack_client,
            backend_mgr
        )
        self._link_manager.link_created.connect(self.new_link)

        self._controllers = {}

        self.update_link_mgr()
        self.update_timer = QTimer()
        self.update_timer.setSingleShot(False)
        self.update_timer.setInterval(1000)
        self.update_timer.timeout.connect(self._link_manager.update)
        self.update_timer.start()

        self._targeted_loop = None

    loopAction = pyqtSignal(int, int, int, list) # track idx, loop idx, LoopActionType value, args
    setPan = pyqtSignal(int, float)
    setVolume = pyqtSignal(int, float)
    
    @pyqtSlot(int, int, int, list)
    def do_loop_action(self, track, loop, action, args):
        self.loopAction.emit(track, loop, action, args)

    @pyqtSlot(AutoconnectRule, MIDIControlLink)
    def new_link(self, rule, link):
        controller = MIDIController(self._autoconnect_rules[rule], self)
        self._controllers[rule] = controller
        controller.loopAction.connect(self.do_loop_action)
        controller.setPan.connect(self.setPan)
        controller.setVolume.connect(self.setVolume)
        link.received.connect(controller.receiveMidi)
        controller.sendMidi.connect(link.send)

        # Send reset sequence
        controller.reset()
        # Send cached states
        if self._active_scene_cache:
            controller.active_scene_changed(self._active_scene_cache)
        if self._scripting_section_cache:
            controller.active_sripting_section_changed(self._scripting_section_cache)
        for ((track, loop), (state, selected, targeted)) in self._loop_states_cache.items():
            controller.loop_state_changed(track, loop, state, selected, targeted)

    @pyqtSlot()
    def update_link_mgr(self):
        link_rules = self._autoconnect_rules.keys()
        self._link_manager.set_rules(link_rules)
    
    @pyqtSlot(int, int, int, bool, bool)
    def loop_state_changed(self, track, index, state, selected, targeted):
        if (track,index) in self._loop_states_cache and \
            self._loop_states_cache[(track,index)] == (state, selected, targeted):
            # Already sent this
            return
        
        for c in self._controllers.values():
            c.loop_state_changed(track, index, state, selected, targeted)
        self._loop_states_cache[(track, index)] = (state, selected, targeted)

    @pyqtSlot(int)
    def active_scripting_section_changed(self, idx):
        if self._active_scene_cache == idx:
            # Already sent this
            return
        
        for c in self._controllers.values():
            c.active_sripting_section_changed(track, index, state)
        self._active_scene_cache = idx
    
    @pyqtSlot(int)
    def active_scene_changed(self, idx):
        if self._scripting_section_cache == idx:
            # Already sent this
            return
        
        for c in self._controllers.values():
            c.active_scene_changed(track, index, state)
        self._scripting_section_cache = idx