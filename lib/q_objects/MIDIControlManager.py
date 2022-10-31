from PyQt6.QtCore import QObject, pyqtSignal, pyqtSlot, QTimer

from ..LoopState import LoopState
from ..MidiScripting import *
from ..flatten import flatten
from .MIDIControlLink import *

from enum import Enum
from typing import *
from pprint import *
from copy import *
import time
from dataclasses import dataclass

class MessageFilterType(Enum):
    Channel = 0
    IsNoteKind = 1 # filter on note messages
    NoteId = 2
    NoteVelocity = 3
    IsNoteOn = 4
    IsNoteOff = 5
    IsNoteShortPress = 6
    IsNoteDoublePress = 7
    IsCCKind = 8 # filter on CC messages
    CCController = 9
    CCValue = 10

class InputRule:
    press_period = 0.5

    def __init__(self,
                 filters : dict[Type[MessageFilterType], Union[None, int]],
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

        if len(msg_bytes) >= 2:
            controller_or_note = msg_bytes[1]
        
        if len(msg_bytes) >= 3:
            value_or_velocity = msg_bytes[2]
        
        if (MessageFilterType.CCController.value in filters or MessageFilterType.CCValue.value in filters) and \
           MessageFilterType.IsCCKind.value not in filters:
           filters[MessageFilterType.IsCCKind.value] = None
        
        if (MessageFilterType.NoteId.value in filters or \
            MessageFilterType.NoteVelocity.value in filters or \
            MessageFilterType.IsNoteOff.value in filters or \
            MessageFilterType.IsNoteOn.value in filters or \
            MessageFilterType.IsNoteShortPress.value in filters or \
            MessageFilterType.IsNoteDoublePress.value in filters) and \
           MessageFilterType.IsNoteKind.value not in filters:
           filters[MessageFilterType.IsNoteKind.value] = None
        
        if type_byte == 0x80:
            print('noteOff {} {}'.format(controller_or_note, value_or_velocity))
            if MessageFilterType.IsCCKind.value in filters or \
                MessageFilterType.IsNoteOn.value in filters or \
                (MessageFilterType.NoteId.value in filters and filters[MessageFilterType.NoteId.value] != controller_or_note) or \
                (MessageFilterType.NoteVelocity.value in filters and filters[MessageFilterType.NoteVelocity.value] != value_or_velocity):
                return False
               
        elif type_byte == 0x90:
            print('noteOn {}'.format(controller_or_note, value_or_velocity))
            if MessageFilterType.IsCCKind.value in filters or \
                MessageFilterType.IsNoteOff.value in filters or \
                (MessageFilterType.NoteId.value in filters and filters[MessageFilterType.NoteId.value] != controller_or_note) or \
                (MessageFilterType.NoteVelocity.value in filters and filters[MessageFilterType.NoteVelocity.value] != value_or_velocity):
                return False

        elif type_byte == 0xB0:
            print('CC {} {}'.format(controller_or_note, value_or_velocity))
            if MessageFilterType.IsNoteKind.value in filters or \
                (MessageFilterType.CCController.value in filters and filters[MessageFilterType.CCController.value] != controller_or_note) or \
                (MessageFilterType.CCValue.value in filters and filters[MessageFilterType.CCValue.value] != value_or_velocity):
                return False
        
        # TODO: presses and double-presses    
        return True

def get_builtin_substitutions(msg_bytes):

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
    # Rules for handling MIDI input messages.
    input_rules: list[Type[InputRule]]
    # Formulas to be executed when loops change to a particular state.
    loop_state_output_formulas: dict[Type[LoopState], str]
    # Formulas to be executed when loops change to a state not covered
    # by loop_state_output_formulas.
    loop_state_default_output_formula: Union[str, None]

builtin_dialects = {
    'AKAI APC Mini': MIDIControlDialect(
        {   # Substitutions to map buttons to loops and vice versa
            'loop_note':  '0 if (track == 0 and loop == 0) else (56+track-1-loop*8)',
            'note_track': '0 if note == 0 else (note % 8 + 1)',
            'note_loop':  '0 if note == 0 else 7 - (note / 8)',
            'fader_track': 'controller-48+1', #'controller-48+1 if controller >= 48 and controller < 56'
        },
        [   # Rules
            InputRule({
                MessageFilterType.IsNoteOn.value: None,
            }, ['note <= 64'], 'loopAction(note_track, note_loop, activate)'),
            InputRule({
                MessageFilterType.IsCCKind.value: None,
            }, ['48 <= controller < 56'], 'setVolume(fader_track, value/127.0)')
        ],
        {
            # Loop states to button colors
            LoopState.Recording.value: 'noteOn(0, loop_note, 3)',
            LoopState.Playing.value: 'noteOn(0, loop_note, 1)',
            LoopState.WaitStart.value: 'noteOn(0, loop_note, 4)',
            LoopState.WaitStop.value: 'noteOn(0, loop_note, 4)',
            LoopState.Off.value: 'noteOn(0, loop_note, 0)',
        },
        # Any unmapped state maps to yellow
        'noteOn(0, loop_note, 5)'
        )
}

# A MIDI Controller object executes the MIDI communication
# for a given dialect.
class MIDIController(QObject):
    def __init__(self, dialect, parent=None):
        super(MIDIController, self).__init__(parent)
        self._dialect = dialect

        self._loop_states_cache = {} # tuple of (track, loop) => last transmitted state
        self._scripting_section_cache = None # last transmitted section
        self._active_scene_cache = None # last transmitted active scene
    
    sendMidi = pyqtSignal(list) # list of byte values
    loopAction = pyqtSignal(int, int, int, list) # track idx, loop idx, LoopActionType value, args
    setPan = pyqtSignal(int, float)
    setVolume = pyqtSignal(int, float)

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
        
        substitutions = {**get_builtin_substitutions(_bytes), **self._dialect.substitutions}
        formulas_to_execute = []
        for r in self._dialect.input_rules:
            if r.apply_filters(_bytes):
                print('filter pass')
                execute = True
                for c in r.condition_formulas:
                    result = eval_formula(c, False, substitutions)
                    print('cond result {} {}'.format(c, result))
                    if not result or not isinstance(result, bool):
                        execute = False
                if execute:
                    formulas_to_execute.append(r.formula)
        
        actions = flatten([eval_formula(f, True, substitutions) for f in formulas_to_execute])
        self.trigger_actions(actions)
    
    @pyqtSlot(int, int, int)
    def loop_state_changed(self, track, index, state):
        if (track, index) in self._loop_states_cache and self._loop_states_cache[(track, index)] == state:
            return # already sent this
        
        if state in self._dialect.loop_state_output_formulas:
            self.send_midi_messages(eval_formula(self._dialect.loop_state_output_formulas[state], True, {**{'track': track, 'loop': index, 'state': state}, **self._dialect.substitutions}))
            self._loop_states_cache[(track, index)] = state
        elif self._dialect.loop_state_default_output_formula:
            self.send_midi_messages(eval_formula(self._dialect.loop_state_default_output_formula, True, {**{'track': track, 'loop': index, 'state': state}, **self._dialect.substitutions}))
            self._loop_states_cache[(track, index)] = state

    @pyqtSlot(int)
    def active_sripting_section_changed(self, idx):
        if self._scripting_section_cache == idx:
            return # already sent this

        raise NotImplementedError()
    
    @pyqtSlot(int)
    def active_scene_changed(self, idx):
        if self._active_scene_cache == idx:
            return # already sent this
        
        raise NotImplementedError()
        

# For communicating with MIDI control/input devices.
class MIDIControlManager(QObject):
    def __init__(self, parent, jack_client, jack_lib_instance):
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

        self._link_manager = MIDIControlLinkManager(
            None,
            jack_client,
            jack_lib_instance
        )
        self._link_manager.link_created.connect(self.new_link)

        self._controllers = {}

        self.update_link_mgr()
        self.update_timer = QTimer()
        self.update_timer.setSingleShot(False)
        self.update_timer.setInterval(1000)
        self.update_timer.timeout.connect(self._link_manager.update)
        self.update_timer.start()

    loopAction = pyqtSignal(int, int, int, list) # track idx, loop idx, LoopActionType value, args
    setPan = pyqtSignal(int, float)
    setVolume = pyqtSignal(int, float)

    @pyqtSlot(AutoconnectRule, MIDIControlLink)
    def new_link(self, rule, link):
        controller = MIDIController(self._autoconnect_rules[rule])
        self._controllers[rule] = controller
        controller.loopAction.connect(self.loopAction)
        controller.setPan.connect(self.setPan)
        controller.setVolume.connect(self.setVolume)
        link.received.connect(controller.receiveMidi)
        controller.sendMidi.connect(link.send)

    @pyqtSlot()
    def update_link_mgr(self):
        link_rules = self._autoconnect_rules.keys()
        self._link_manager.set_rules(link_rules)
    
    @pyqtSlot(int, int, int)
    def loop_state_changed(self, track, index, state):
        for c in self._controllers.values():
            c.loop_state_changed(track, index, state)

    @pyqtSlot(int)
    def active_sripting_section_changed(self, idx):
        for c in self._controllers.values():
            c.active_sripting_section_changed(track, index, state)
    
    @pyqtSlot(int)
    def active_scene_changed(self, idx):
        for c in self._controllers.values():
            c.active_scene_changed(track, index, state)