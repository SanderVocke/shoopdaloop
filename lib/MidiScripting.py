from enum import Enum
from typing import Callable, Type, Union
import functools
import ast
import operator
from pprint import pformat

from .LoopState import LoopActionType
from .flatten import flatten

class MIDIMessage:
    def __init__(self):
        pass

    def bytes(self):
        return []

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

class ParseError(Exception):
    pass

class LoopAction:
    def __init__(self, action_type, track_idx, loop_idx, args):
        self.action_type = action_type
        self.track_idx = track_idx
        self.loop_idx = loop_idx
        self.args = args

    def __eq__(self, other):
        return self.action_type == other.action_type and \
               self.track_idx == other.track_idx and \
               self.loop_idx == other.loop_idx and \
               self.args == other.args

def eval_formula(formula: str, substitutions: dict[str, str] = {}) -> list[MIDIMessage]:
    def eval_noteOn(arg_nodes : list[ast.Expr]):
        if len(arg_nodes) != 3:
            raise ParseError('noteOn takes 3 arguments (' + str(len(arg_nodes)) + ' given)')
        
        channel = eval_expr(arg_nodes[0])
        note = eval_expr(arg_nodes[1])
        velocity = eval_expr(arg_nodes[2])

        if type(channel) is not int and type(channel) is not float:
            raise ParseError('Could not evaluate noteOn channel value of type ' + str(type(channel)))
        if type(note) is not int and type(note) is not float:
            raise ParseError('Could not evaluate noteOn note value of type ' + str(type(note)))
        if type(velocity) is not int and type(velocity) is not float:
            raise ParseError('Could not evaluate noteOn velocity value of type ' + str(type(note)))
        
        return [ MIDINoteMessage(int(channel), int(note), True, int(velocity)) ]
    
    def eval_noteOff(arg_nodes : list[ast.Expr]):
        if len(arg_nodes) != 3:
            raise ParseError('noteOff takes 3 arguments (' + str(len(arg_nodes)) + ' given)')
        
        channel = eval_expr(arg_nodes[0])
        note = eval_expr(arg_nodes[1])
        velocity = eval_expr(arg_nodes[2])

        if type(channel) is not int and type(channel) is not float:
            raise ParseError('Could not evaluate noteOff channel value of type ' + str(type(channel)))
        if type(note) is not int and type(note) is not float:
            raise ParseError('Could not evaluate noteOff note value of type ' + str(type(note)))
        if type(velocity) is not int and type(velocity) is not float:
            raise ParseError('Could not evaluate noteOff velocity value of type ' + str(type(note)))
        
        return [ MIDINoteMessage(int(channel), int(note), False, int(velocity)) ]
    
    def eval_cc(arg_nodes : list[ast.Expr]):
        if len(arg_nodes) != 3:
            raise ParseError('cc takes 3 arguments (' + str(len(arg_nodes)) + ' given)')
        
        channel = eval_expr(arg_nodes[0])
        controller = eval_expr(arg_nodes[1])
        value = eval_expr(arg_nodes[2])

        if type(channel) is not int and type(channel) is not float:
            raise ParseError('Could not evaluate cc channel value of type ' + str(type(channel)))
        if type(controller) is not int and type(controller) is not float:
            raise ParseError('Could not evaluate cc controller value of type ' + str(type(controller)))
        if type(value) is not int and type(value) is not float:
            raise ParseError('Could not evaluate cc value value of type ' + str(type(value)))
        
        return [ MIDICCMessage(int(channel), int(controller), int(value)) ]
    
    def eval_loopAction(arg_nodes : list[ast.Expr]):
        if len(arg_nodes) < 3:
            raise ParseError('loopAction takes at least 3 arguments ({} given}'.format(len(arg_nodes)))
        
        track_idx = eval_expr(arg_nodes[0])
        loop_idx = eval_expr(arg_nodes[1])
        action_node = arg_nodes[2]

        if not isinstance(action_node, ast.Name):
            raise ParseError('Expected identifier for loopAction, got {}'.format(str(type(action_node))))
        action_name = action_node.id
        if action_name not in loop_action_names:
            raise ParseError('Unknown loop action: {}'.format(action_name))
        
        action_type = loop_action_names[action_name]

        args = []
        for n in arg_nodes[3:]:
            args.append(eval_expr(n))
        
        if type(track_idx) is not int and type(track_idx) is not float:
            raise ParseError('Could not evaluate track idx of loopAction (type {})'.format(str(type(track_idx))))
        if type(loop_idx) is not int and type(loop_idx) is not float:
            raise ParseError('Could not evaluate loop idx of loopAction (type {})'.format(str(type(loop_idx))))
        
        return [ LoopAction(action_type, int(track_idx), int(loop_idx), args) ]

    def eval_stmt_call(node : ast.Call):
        if not isinstance(node.func, ast.Name):
            raise ParseError('Invalid syntax')
        
        calls = {
            'noteOn': eval_noteOn,
            'noteOff': eval_noteOff,
            'cc': eval_cc,
            'loopAction': eval_loopAction,
        }

        for name, fn in calls.items():
            if node.func.id == name:
                return fn(node.args)

        raise ParseError('Unknown call: "' + node.func.id + '"')

    def eval_constant(node):
        return node.value
    
    def eval_name(node):
        if node.id in substitutions:
            m = ast.parse(str(substitutions[node.id])).body[0]
            if not isinstance(m, ast.Expr):
                raise ParseError('Failed to evaluate: ' + str(substitutions[node.id]))
            return eval_expr(m.value)
        
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
            
        raise ParseError(node.ops[0])

    def eval_binop(node):
        ops = {
            ast.Add: operator.add,
            ast.Sub: operator.sub,
            ast.Mult: operator.mul,
            ast.Div: operator.truediv,
            ast.NotEq: lambda a, b: not operator.eq(a,b)
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

    node = ast.parse(formula)

    if not isinstance(node, ast.Module):
        raise ParseError('Failed to parse')
    
    return flatten(eval_stmt(node.body))

def test():
    def check_eq(a, b):
        if (a != b):
            raise Exception("{} != {}".format(pformat(a), pformat(b)))
    check_eq(eval_formula('noteOn(1,2,100)', {}), [ MIDINoteMessage(1, 2, True, 100) ])
    check_eq(eval_formula('noteOff(1,2,100)', {}), [ MIDINoteMessage(1, 2, False, 100) ])
    check_eq(eval_formula('cc(1,2,5)', {}), [ MIDICCMessage(1, 2, 5) ])
    check_eq(eval_formula('noteOn(a,b,100)', {'a': 1, 'b': 2}), [ MIDINoteMessage(1, 2, True, 100)])
    check_eq(eval_formula('noteOn(1, 2, 3) if 1 else noteOn(3, 2, 1)', {}), [ MIDINoteMessage(1, 2, True, 3)])
    check_eq(eval_formula('noteOn(1, 2, 3) if 0 else noteOn(3, 2, 1)', {}), [ MIDINoteMessage(3, 2, True, 1)])
    check_eq(eval_formula('loopAction(1, 2, play)', {}), [ LoopAction(LoopActionType.Play.value, 1, 2, []) ])
    check_eq(eval_formula('loopAction(1, 2, recordN, 10)', {}), [ LoopAction(LoopActionType.RecordNCycles.value, 1, 2, [10]) ])