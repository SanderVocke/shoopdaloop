from enum import Enum
from typing import Callable, Type, Union
import functools
import ast
import operator
from pprint import pformat
from dataclasses import dataclass
import pprint

from .StatesAndActions import *
from .flatten import flatten

class ScriptingAction:
    def __init__(self):
        pass

class MIDIMessage(ScriptingAction):
    def __init__(self):
        ScriptingAction.__init__(self)

    def bytes(self):
        return []
    
    def __str__(self):
        return ''

class MIDINoteMessage(MIDIMessage):
    def __init__(self, channel : int, note : int, on : bool, velocity: int):
        MIDIMessage.__init__(self)
        self._channel = channel
        self._note = note
        self._on = on
        self._velocity = velocity
    
    def __eq__(self, other):
        return self._channel == other._channel and \
               self._note == other._note and \
               self._on == other._on and \
               self._velocity == other._velocity
    
    def bytes(self):
        return [(0x90 if self._on else 0x80) + self._channel, self._note, self._velocity]
    
    def __str__(self):
        return 'note ' + ('on' if self._on else 'off') + ', chan {}, note {}, vel {}'.format(
            self._channel, self._note, self._velocity
        )

class MIDICCMessage(MIDIMessage):
    def __init__(self, channel : int, controller : int, value : int):
        MIDIMessage.__init__(self)
        self._channel = channel
        self._controller = controller
        self._value = value
    
    def __eq__(self, other):
        return self._channel == other._channel and \
               self._controller == other._controller and \
               self._value == other._value
    
    def bytes(self):
        return [0xB0 + self._channel, self._controller, self._value]
    
    def __str__(self):
        return 'CC chan {}, controller {}, value {}'.format(
            self._channel, self._controller, self._value
        )

class LoopAction(ScriptingAction):
    def __init__(self, action_type, track_idx, loop_idx, args, propagate_to_selected):
        ScriptingAction.__init__(self)
        self.action_type = action_type
        self.track_idx = track_idx
        self.loop_idx = loop_idx
        self.args = args
        self.propagate_to_selected = propagate_to_selected

    def __eq__(self, other):
        return self.action_type == other.action_type and \
               self.track_idx == other.track_idx and \
               self.loop_idx == other.loop_idx and \
               self.args == other.args and \
               self.propagate_to_selected == other.propagate_to_selected
    
    def __str__(self):
        return 'Loop action: type {}, track {}, loop {}, args {}, propagate {}'.format(
            self.action_type,
            self.track_idx,
            self.loop_idx,
            pformat(self.args),
            self.propagate_to_selected
        )


class TrackAction(ScriptingAction):
    def __init__(self, track_idx, action_type, args):
        self.action_type = action_type
        self.track_idx = track_idx
        self.args = args
    
    def __eq__(self, other):
        return self.action_type == other.action_type and \
               self.track_idx == other.track_idx and \
               self.args == other.args
    
    def __str__(self):
        return 'Track action: type {}, track {}, args {}'.format(
            self.action_type,
            self.track_idx,
            pformat(self.args)
        )

class ParseError(Exception):
    pass

loop_action_names = {
    LoopActionType.names.value[key]: key for key in LoopActionType.names.value.keys()
}

supported_calls = {
    'noteOn': {
        'is_stmt': True,
        'args': [
            {'name': 'channel', 'type': 'num' },
            {'name': 'note', 'type': 'num' },
            {'name': 'velocity', 'type': 'num' },
        ],
        'may_have_additional_args': False,
        'description': 'Send a MIDI NoteOn message.',
        'evaluator': lambda helper_functions: lambda channel, note, velocity: [ 
            MIDINoteMessage(int(channel), int(note), True, int(velocity))
            ]
    },
    'noteOff': {
        'is_stmt': True,
        'args': [
            {'name': 'channel', 'type': 'num' },
            {'name': 'note', 'type': 'num' },
            {'name': 'velocity', 'type': 'num' },
        ],
        'may_have_additional_args': False,
        'description': 'Send a MIDI NoteOff message.',
        'evaluator':   lambda helper_functions: lambda channel, note, velocity: [
            MIDINoteMessage(int(channel), int(note), False, int(velocity))
            ]
    },
    'notesOn': {
        'is_stmt': True,
        'args': [
            {'name': 'channel', 'type': 'num' },
            {'name': 'firstNote', 'type': 'num' },
            {'name': 'lastNote', 'type': 'num' },
            {'name': 'velocity', 'type': 'num' },
        ],
        'may_have_additional_args': False,
        'description': 'Send a MIDI NoteOn message for all notes in a range.',
        'evaluator':   lambda helper_functions: lambda channel, firstNote, lastNote, velocity: [
            MIDINoteMessage(int(channel), int(note), True, int(velocity)) for note in range(firstNote, lastNote+1)
            ]
    },
    'notesOff': {
        'is_stmt': True,
        'args': [
            {'name': 'channel', 'type': 'num' },
            {'name': 'firstNote', 'type': 'num' },
            {'name': 'lastNote', 'type': 'num' },
            {'name': 'velocity', 'type': 'num' },
        ],
        'may_have_additional_args': False,
        'description': 'Send a MIDI NoteOff message for all notes in a range.',
        'evaluator':   lambda helper_functions: lambda channel, firstNote, lastNote, velocity: [
            MIDINoteMessage(int(channel), int(note), False, int(velocity)) for note in range(firstNote, lastNote+1)
            ]
    },
    'cc': {
        'is_stmt': True,
        'args': [
            {'name': 'channel', 'type': 'num' },
            {'name': 'note', 'type': 'num' },
            {'name': 'velocity', 'type': 'num' },
        ],
        'may_have_additional_args': False,
        'description': 'Send a MIDI CC message.',
        'evaluator':   lambda helper_functions: lambda channel, controller, value: [
            MIDICCMessage(int(channel), int(controller), int(value))
            ]
    },
    'loopAction': {
        'is_stmt': True,
        'args': [
            {'name': 'track_idx', 'type': 'num' },
            {'name': 'loop_idx', 'type': 'num' },
            {'name': 'action', 'type': 'loop_action_str' },
            {'name': 'propagate_to_selected', 'type': 'bool'}
        ],
        'may_have_additional_args': True,
        'description': 'Perform an action on a loop.',
        'evaluator':   lambda helper_functions: lambda track_idx, loop_idx, action, propagate_to_selected, rest: [
            LoopAction(action, int(track_idx), int(loop_idx), rest, bool(propagate_to_selected))
            ]
    },
    'setVolume': {
        'is_stmt': True,
        'args': [
            {'name': 'track_idx', 'type': 'num' },
            {'name': 'value', 'type': 'num' },
        ],
        'may_have_additional_args': False,
        'description': 'Set the volume of a track.',
        'evaluator':   lambda helper_functions: lambda track_idx, value: [
            TrackAction(int(track_idx), PortActionType.SetPortVolume, [float(value)])
            ]
    },
    'set': {
        'is_stmt': True,
        'args': [
            {'name': 'name', 'type': 'string' },
            {'name': 'value', 'type': 'any' },
        ],
        'may_have_additional_args': False,
        'description': 'Set a variable.',
        'evaluator':   lambda helper_functions: lambda name, value: helper_functions.set_var(name, value)
    },
    'get': {
        'is_stmt': False,
        'args': [
            {'name': 'name', 'type': 'string' },
        ],
        'may_have_additional_args': False,
        'description': 'Get a variable.',
        'evaluator':   lambda helper_functions: lambda name: helper_functions.get_var(name)
    },
}

def eval_formula(formula: str,
                 is_stmt: bool,
                 substitutions: dict[str, str] = {},
                 notes_currently_on: list[tuple[int,int]] = [],
                 helper_functions: dict[str, Callable] = {},
                ) -> list[ScriptingAction]:

    def eval_call(node : ast.Call, is_stmt: bool):
        if not isinstance(node.func, ast.Name):
            raise ParseError('Invalid syntax')
        
        calls = {s: supported_calls[s] for s in supported_calls.keys() if supported_calls[s]['is_stmt'] == is_stmt}
        
        if not node.func.id in calls.keys():
            raise ParseError('Unknown function: "' + node.func.id + '"')
        call_desc = calls[node.func.id]
        args = [eval_expr(arg) for arg in node.args]
        
        if len(args) < len(call_desc['args']) or \
            (len(args) > len(call_desc['args']) and not call_desc['may_have_additional_args']):
            raise ParseError('Call {} received {} arguments, but got {}'.format(
                node.func.id,
                len(call_desc['args']),
                len(args)
                ))

        def parse_arg (arg, idx):
            arg_desc = call_desc['args'][idx]
            match arg_desc['type']:
                case 'num':
                    if type(arg) is not int and type(arg) is not float:
                        raise ParseError('Expected argument {} to call {} to be of numeric type, but it was of type {} (value {})'.format(
                            idx,
                            node.func.id,
                            type(arg),
                            arg
                        ))
                    return arg
                case 'loop_action_str':
                    if arg not in loop_action_names:
                        raise ParseError('Unknown loop action {} passed to call {}'.format(
                            arg,
                            node.func.id
                        ))
                    action_type = loop_action_names[arg]
                    return action_type
                case 'string':
                    return str(arg)
                case 'any':
                    return arg
                case 'bool':
                    return bool(arg)
                case other:
                    raise ParseError('Unknown argument type: "{}"'.format(arg_desc['type']))
        
        parsed_args = [parse_arg(a, idx) for idx,a in enumerate(args[:len(call_desc['args'])])]
        if call_desc['may_have_additional_args']:
            parsed_args.append(args[len(call_desc['args']):])

        return call_desc['evaluator'](helper_functions)(*parsed_args)
    
    def eval_stmt_call(node):
        return eval_call(node, True)
    
    def eval_expr_call(node):
        if not isinstance(node.func, ast.Name):
              raise ParseError('Invalid syntax')
        
        if node.func.id == 'isNotePressed':
            args = [eval_expr(arg) for arg in node.args]
            if len(args) != 2:
                raise ParseError('Call isNotePressed expects 2 arguments, but got {}'.format(len(args)))
            channel = int(args[0])
            note = int(args[1])
            return (channel, note) in notes_currently_on

        return eval_call(node, False)

    def eval_constant(node):
        return node.value
    
    def eval_name(node):
        if node.id in substitutions:
            m = ast.parse(str(substitutions[node.id])).body[0]
            if not isinstance(m, ast.Expr):
                raise ParseError('Failed to evaluate: ' + str(substitutions[node.id]))
            return eval_expr(m.value)
        
        maybe_var = get_var(node.id)
        if maybe_var is not None:
            return maybe_var
        
        raise ParseError('Unknown identifier: "{}"'.format(node.id))
    
    def eval_bool_op(node):
        if isinstance(node.op, ast.And):
            return functools.reduce(lambda a, b: eval_expr(a) and eval_expr(b), node.values)
        elif isinstance(node.op, ast.Or):
            return functools.reduce(lambda a, b: eval_expr(a) or eval_expr(b), node.values)
            
        raise ParseError(node.op)
    
    def eval_compare(node):
        if len(node.ops) == 0:
            return eval_expr(node.left)

        if isinstance(node.ops[0], ast.Eq):
            return eval_expr(node.left) == eval_compare(ast.Compare(node.comparators[0], node.ops[1:], node.comparators[1:]))
        elif isinstance(node.ops[0], ast.NotEq):
            return eval_expr(node.left) != eval_compare(ast.Compare(node.comparators[0], node.ops[1:], node.comparators[1:]))
        elif isinstance(node.ops[0], ast.Lt):
            return eval_expr(node.left) < eval_compare(ast.Compare(node.comparators[0], node.ops[1:], node.comparators[1:]))
        elif isinstance(node.ops[0], ast.Gt):
            return eval_expr(node.left) > eval_compare(ast.Compare(node.comparators[0], node.ops[1:], node.comparators[1:]))
        elif isinstance(node.ops[0], ast.LtE):
            return eval_expr(node.left) <= eval_compare(ast.Compare(node.comparators[0], node.ops[1:], node.comparators[1:]))
        elif isinstance(node.ops[0], ast.GtE):
            return eval_expr(node.left) >= eval_compare(ast.Compare(node.comparators[0], node.ops[1:], node.comparators[1:]))

        raise ParseError(node.ops[0])

    def eval_binop(node):
        ops = {
            ast.Add: operator.add,
            ast.Sub: operator.sub,
            ast.Mult: operator.mul,
            ast.Div: operator.truediv,
            ast.NotEq: lambda a, b: not operator.eq(a,b),
            ast.Mod: operator.mod,
            ast.FloorDiv: operator.floordiv,
        }

        left_value = eval_expr(node.left)
        right_value = eval_expr(node.right)
        apply = ops[type(node.op)]
        return apply(left_value, right_value)

    def eval_unaryop(node):
        ops = {
            ast.USub: operator.neg,
            ast.Not: operator.not_,
        }

        operand_value = eval_expr(node.operand)
        apply = ops[type(node.op)]
        return apply(operand_value)

    def eval_expr(node):
        evaluators = {
            ast.BinOp: eval_binop,
            ast.UnaryOp: eval_unaryop,
            ast.Constant: eval_constant,
            ast.Name: eval_name,
            ast.BoolOp: eval_bool_op,
            ast.Compare: eval_compare,
            ast.IfExp: eval_if_expr,
            ast.Call: eval_expr_call
        }

        for ast_type, evaluator in evaluators.items():
            if isinstance(node, ast_type):
                return evaluator(node)

        raise ParseError('Invalid expression syntax', node)
    
    def eval_if_expr(node):
        test_result = eval_expr(node.test)
        if test_result:
            return eval_expr(node.body)
        elif node.orelse:
            return eval_expr(node.orelse)
    
    def eval_if_stmt(node):
        test_result = eval_expr(node.test)
        if test_result:
            return eval_stmt(node.body)
        elif node.orelse:
            return eval_stmt(node.orelse)
    
    def eval_stmt(node):
        def eval_stmts(nodes):
            return [eval_stmt(n) for n in nodes]

        def eval_stmt_expr(node):
            if isinstance(node.value, ast.Call):
                return eval_stmt_call(node.value)
            elif isinstance(node.value, ast.IfExp):
                return eval_if_stmt(node.value)

        evaluators = {
            ast.Call: eval_stmt_call,
            ast.Expr: eval_stmt_expr,
            ast.IfExp: eval_if_stmt,
            list: eval_stmts,
        }

        for ast_type, evaluator in evaluators.items():
            if isinstance(node, ast_type):
                return evaluator(node)

        raise ParseError('Invalid statement syntax', node)

    try:
        node = ast.parse(formula)
        if not isinstance(node, ast.Module):
            raise ParseError('Failed to parse')
    
        if is_stmt:
            return flatten(eval_stmt(node.body))
        else:
            return eval_expr(node.body[0].value)

    except Exception as e:
        print("Failed to parse formula '{}': {}".format(formula, str(e)))
        return []

def test():
    def check_eq(a, b):
        if (a != b):
            raise Exception("{} != {}".format(pformat([str(x) for x in a]), pformat([str(x) for x in b])))
    check_eq(eval_formula('noteOn(1,2,100)', True), [ MIDINoteMessage(1, 2, True, 100) ])
    check_eq(eval_formula('noteOff(1,2,100)', True), [ MIDINoteMessage(1, 2, False, 100) ])
    check_eq(eval_formula('cc(1,2,5)', True), [ MIDICCMessage(1, 2, 5) ])
    check_eq(eval_formula('noteOn(a,b,100)', True, {'a': 1, 'b': 2}), [ MIDINoteMessage(1, 2, True, 100)])
    check_eq(eval_formula('noteOn(1, 2, 3) if 1 else noteOn(3, 2, 1)', True), [ MIDINoteMessage(1, 2, True, 3)])
    check_eq(eval_formula('noteOn(1, 2, 3) if 0 else noteOn(3, 2, 1)', True), [ MIDINoteMessage(3, 2, True, 1)])
    check_eq(eval_formula('loopAction(1, 2, "play", False)', True), [ LoopAction(LoopActionType.DoPlay.value, 1, 2, [], False) ])
    check_eq(eval_formula('loopAction(1, 2, "recordN", True, 10)', True), [ LoopAction(LoopActionType.DoRecordNCycles.value, 1, 2, [10], True) ])
    check_eq(eval_formula('loopAction(1, 2, "record" if isNotePressed(0, 0) else "play", True)', True, {}, [(0,0)]),
                            [ LoopAction(LoopActionType.DoRecord.value, 1, 2, [], True) ])
    check_eq(eval_formula('loopAction(1, 2, "record" if isNotePressed(0, 0) else "play", False)', True, {}, [(0,1)]),
                            [ LoopAction(LoopActionType.DoPlay.value, 1, 2, [], False) ])
    
    class TestVars():
        def __init__(self):
            self.vars = {}
        def set_var(self, name, value):
            self.vars[name] = value
        def get_var(self, name):
            return self.vars[name]
    helpers = {
        'get_var': lambda name: vars.get_var(name),
        'set_var': lambda name, value: vars.set_var(name, value)
    }

    # A small sequence with variables
    vars = TestVars()
    eval_formula('set("my_var", 5)', True, helper_functions=helpers)
    check_eq(vars.get_var('my_var'), 5)
    check_eq(eval_formula('noteOn(1, get("my_var"), 127 if get("my_var") > 5 else 126)', True, helper_functions=helpers),
        [ MIDINoteMessage(1, 5, True, 126) ])