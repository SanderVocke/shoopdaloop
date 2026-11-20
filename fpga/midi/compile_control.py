#!/usr/bin/env python3
"""Compile the bounded MIDI control program into a shared-datapath instruction ROM.

Generation needs pyslang 12.1.0. Normal RTL builds use the checked-in generated ROM.
The input is a deliberately restricted sequential SystemVerilog subset, not a
replacement for a general HDL compiler.
"""

from __future__ import annotations
from dataclasses import dataclass
import json
import hashlib
from importlib.metadata import version
from pathlib import Path
import re

from pyslang.ast import Compilation
from pyslang.syntax import SyntaxTree

if version("pyslang") != "12.1.0":
    raise RuntimeError("MIDI control generation requires pyslang==12.1.0")

ROOT = Path(__file__).resolve().parent
OPS = [
    "NOP",
    "MOV",
    "MOVI",
    "MOVHI",
    "ADD",
    "SUB",
    "AND",
    "OR",
    "XOR",
    "SHL",
    "SHR",
    "EQ",
    "NE",
    "LT",
    "LE",
    "GT",
    "GE",
    "BOOL",
    "NOT",
    "INV",
    "MUL32",
    "DIV",
    "MOD",
    "JUMP",
    "JZERO",
    "JIND",
    "LOAD",
    "STORE",
    "IOREAD",
    "IOWRITE",
    "COMMAND",
    "REPLY",
    "MEMREAD",
    "MEMWRITE",
    "CONTENT",
    "HEADREAD",
    "HEADWRITE",
    "PACKET",
    "PKTERR",
    "FILL",
    "FIRSTBIT",
    "CTXREAD",
    "CTXWRITE",
    "BULKCOPY",
    "BULKZERO",
    "BULKIN",
    "BULKOUT",
    "CTXZERO",
    "CLOAD",
    "CSTORE",
    "MERGE",
]
OP = {name: i for i, name in enumerate(OPS)}
INPUTS = [
    "arena_base",
    "quantum_frames",
    "op",
    "a0",
    "a1",
    "a2",
    "a3",
    "a4",
    "data_word",
    "last_error",
    "head_word",
    "packet_refused",
]
# Values captured or consumed by independent ready/valid kernels.
OUTPUTS = {
    "response_error": "4",
    "response_handle": "64",
    "response_frame": "64",
    "response_count": "32",
    "response_packet": "24",
    "response_length": "2",
    "packet_destination": "32",
    "packet_offset": "32",
    "own_address": "64",
    "own_data": "32",
    "send_packet": "24",
    "send_length": "2",
    "head_write_index": "LOOP_BITS",
    "head_write_data": "HEAD_BITS",
    "prepared": "1",
    "running": "1",
}
IO_NAMES = list(dict.fromkeys(INPUTS + list(OUTPUTS)))
IO_ID = {name: i for i, name in enumerate(IO_NAMES)}


@dataclass(frozen=True)
class Value:
    kind: str
    data: str
    number: int | None = None
    width: int | str = 64


def constant(n: int) -> Value:
    n &= (1 << 64) - 1
    return Value("c", f"64'h{n:016x}", n, max(1, n.bit_length()))


def key(symbol):
    return str(symbol.hierarchicalPath)


def kind(node):
    return type(node).__name__


class Compiler:
    def __init__(self, content=False):
        self.content = content
        self.prefix = "S_" if content else ""
        self.pc_name = "phase" if content else "pc"
        self.inputs = {"arena_base", "data_word"} if content else set(INPUTS)
        self.output_alias = (
            {"memory_address": "own_address", "memory_write_data": "own_data"}
            if content
            else {n: n for n in OUTPUTS}
        )
        self.external_values = {}
        self.command_values = {}
        self.other_body = None
        self.constants = {}
        self.code = []
        self.labels = {}
        self.serial = 0
        self.symbols = {}
        self.banks = []
        self.temp = 0
        self.peak_temp = 0
        self.io_cache = {}
        self.head_commit = False
        self.array_names = {}
        program = (
            ROOT
            / (
                "program/midi_content_control.sv"
                if content
                else "program/midi_job_control.sv"
            )
        ).read_text()
        header = (ROOT / "rtl/midi_functions.svh").read_text()
        text = program.replace('`include "midi_functions.svh"', header)
        if not content:
            text += "\n" + (ROOT / "rtl/midi_store.sv").read_text().replace('`include "midi_functions.svh"', header)
        self.compilation = Compilation()
        self.compilation.addSyntaxTree(SyntaxTree.fromText(text))
        self.body = next(
            i.body
            for i in self.compilation.getRoot().topInstances
            if i.name == ("midi_content_control" if content else "midi_job_control")
        )
        if self.compilation.hasIssuedErrors:
            raise ValueError(
                "Invalid control program: "
                + str(list(self.compilation.getAllDiagnostics()))
            )
        proc = next(
            s
            for s in self.body
            if s.kind.name == "ProceduralBlock" and "begin : execution" in str(s.syntax)
        )
        conditional = next(
            n
            for n in self.flatten(
                proc.body.stmt if hasattr(proc.body, "stmt") else proc.body
            )
            if kind(n) == "ConditionalStatement"
        )
        self.cases = conditional.ifFalse
        assert kind(self.cases) == "CaseStatement"
        self.states = {}
        for item in self.cases.items:
            assert len(item.expressions) == 1
            expr = item.expressions[0]
            while kind(expr) == "ConversionExpression":
                expr = expr.operand
            self.states[expr.symbol.name] = item.stmt
        contracts = json.loads((ROOT / "program/lowering-contracts.json").read_text())
        for name, expected in contracts["content" if content else "jobs"].items():
            actual = hashlib.sha256(
                re.sub(r"\s+", "", str(self.states[name].syntax)).encode()
            ).hexdigest()
            if actual != expected:
                raise ValueError(
                    "Specialized lowering changed: "
                    + self.prefix
                    + name
                    + "; review the execution contract before updating its fingerprint"
                )
        self.state_names = set(self.states)
        for symbol in self.body:
            if (
                symbol.kind.name == "Variable"
                and symbol.name not in self.inputs
                and symbol.name
                not in [self.pc_name, "heads", "context_words", "head_write"]
            ):
                self.allocate(symbol)

        # Block-local temporaries have ordinary register lifetimes in the program.
        def locals_(node):
            if kind(node) == "VariableDeclStatement":
                self.allocate(node.symbol)

        proc.body.visit(locals_)

    def allocate(self, symbol):
        if key(symbol) in self.symbols:
            return self.symbols[key(symbol)]
        typ = symbol.type
        name = symbol.name
        size = "1"
        if typ.isUnpackedArray:
            size = {
                "c": "32",
                "h": "24",
                "desc": "16",
                "payload": "4",
                "density": "INPUTS",
            }.get(name)
            if size is None:
                raise ValueError("Unexpected array " + name)
            self.array_names[key(symbol)] = size
        elif typ.bitWidth > 64:
            assert name == "target_seen" and typ.bitWidth == 128
            size = "2"
        ident = "R_" + re.sub(
            r"\W",
            "_",
            self.prefix
            + key(symbol).removeprefix(
                "midi_content_control." if self.content else "midi_job_control."
            ),
        )
        result = Value("r", ident, width=min(64, typ.bitWidth or 64))
        self.symbols[key(symbol)] = result
        self.banks.append((ident, size, name))
        return result

    def fresh(self):
        result = Value("r", f"R_TEMP+{self.temp}")
        self.temp += 1
        self.peak_temp = max(self.peak_temp, self.temp)
        return result

    def label(self, prefix="branch"):
        self.serial += 1
        return f"{self.prefix}{prefix}_{self.serial}"

    def mark(self, label):
        assert label not in self.labels, label
        self.labels[label] = len(self.code)

    def emit(self, op, rd=None, a=None, b=None, imm=None, target=None):
        self.code.append(
            dict(
                op=op,
                rd=rd.data if rd else "0",
                a=a.data if a else "0",
                b=b.data if b else "0",
                imm=imm,
                target=target,
            )
        )

    def reg(self, value):
        if value.kind == "r":
            return value
        if value.number == 0:
            return Value("r", "0", width=1)
        if value.kind == "c":
            if value.data not in self.constants:
                self.constants[value.data] = (len(self.constants), value)
            return Value(
                "r", "R_CONST_" + str(self.constants[value.data][0]), width=value.width
            )
        out = self.fresh()
        out = Value(out.kind, out.data, width=value.width)
        if value.kind == "label":
            self.emit("MOVI", out, imm="@" + value.data)
        else:
            self.emit("MOVI", out, imm=f"32'({value.data})")
            if value.number is None or value.number >> 32:
                self.emit("MOVHI", out, a=out, imm=f"32'(({value.data}) >> 32)")
        return out

    def unary(self, op, value):
        if value.kind == "c":
            if value.number is not None:
                n = value.number
                return constant(
                    int(n != 0)
                    if op == "BOOL"
                    else int(n == 0) if op == "NOT" else ~n if op == "INV" else n
                )
            expression = {
                "BOOL": f"({value.data} != 0)",
                "NOT": f"({value.data} == 0)",
                "INV": f"~64'({value.data})",
                "MOV": value.data,
            }[op]
            return Value("c", expression, width=1 if op in ["BOOL", "NOT"] else 64)
        out = self.fresh()
        out = Value(out.kind, out.data, width=1 if op in ["BOOL", "NOT"] else 64)
        self.emit(op, out, self.reg(value))
        return out

    def binary(self, op, a, b):
        if a.kind == b.kind == "c" and a.number is not None and b.number is not None:
            x, y = a.number, b.number
            calc = {
                "ADD": lambda: x + y,
                "SUB": lambda: x - y,
                "AND": lambda: x & y,
                "OR": lambda: x | y,
                "XOR": lambda: x ^ y,
                "SHL": lambda: x << y if y < 64 else 0,
                "SHR": lambda: x >> y if y < 64 else 0,
                "EQ": lambda: int(x == y),
                "NE": lambda: int(x != y),
                "LT": lambda: int(x < y),
                "LE": lambda: int(x <= y),
                "GT": lambda: int(x > y),
                "GE": lambda: int(x >= y),
                "MUL32": lambda: x * y,
                "DIV": lambda: x // y,
                "MOD": lambda: x % y,
            }
            if op in calc:
                return constant(calc[op]())
        if op in ["ADD", "OR", "XOR"] and b.number == 0:
            return a
        if op in ["ADD", "OR", "XOR"] and a.number == 0:
            return b
        if op in ["SHL", "SHR"] and a.number == 0:
            return constant(0)
        if op in ["MUL32", "AND"] and (a.number == 0 or b.number == 0):
            return constant(0)
        if (
            op in ["DIV", "MOD"]
            and b.number is not None
            and b.number > 0
            and b.number & (b.number - 1) == 0
        ):
            return (
                self.binary("SHR", a, constant(b.number.bit_length() - 1))
                if op == "DIV"
                else self.binary("AND", a, constant(b.number - 1))
            )
        if op == "AND" and b.number == (1 << 64) - 1:
            return a
        if op in ["SHL", "SHR"] and b.number == 0:
            return a
        width = 64
        if op in ["EQ", "NE", "LT", "LE", "GT", "GE"]:
            width = 1
        elif op == "AND" and isinstance(a.width, int) and isinstance(b.width, int):
            width = min(a.width, b.width)
        elif (
            op in ["OR", "XOR"]
            and isinstance(a.width, int)
            and isinstance(b.width, int)
        ):
            width = max(a.width, b.width)
        if a.kind == b.kind == "c":
            operator = {
                "ADD": "+",
                "SUB": "-",
                "AND": "&",
                "OR": "|",
                "XOR": "^",
                "SHL": "<<",
                "SHR": ">>",
                "EQ": "==",
                "NE": "!=",
                "LT": "<",
                "LE": "<=",
                "GT": ">",
                "GE": ">=",
                "MUL32": "*",
                "DIV": "/",
                "MOD": "%",
            }[op]
            return Value("c", f"(64'({a.data}) {operator} 64'({b.data}))", width=width)
        if op == "MUL32" and (
            not isinstance(a.width, int)
            or not isinstance(b.width, int)
            or a.width > 32
            or b.width > 32
        ):
            raise ValueError(
                "Control program multiplication operands must fit 32 bits: "
                + repr((a, b))
            )
        if op in ["DIV", "MOD"] and b.width > 32:
            raise ValueError(
                "Control division requires a denominator of at most 32 bits"
            )
        out = self.fresh()
        out = Value(out.kind, out.data, width=width)
        self.emit(op, out, self.reg(a), self.reg(b))
        return out

    def mask(self, value, width):
        if isinstance(width, int):
            if width >= 64 or isinstance(value.width, int) and value.width <= width:
                return value
            if value.kind == "c" and value.number is not None:
                return constant(value.number & ((1 << width) - 1))
            m = constant((1 << width) - 1)
        else:
            identifiers = set(re.findall("[A-Z_]+", width))
            bounded = bool(identifiers) and identifiers <= {
                "LOOP_BITS",
                "INPUT_BITS",
                "DEST_BITS",
                "VIEW_BITS",
                "TABLE_BITS",
            }
            m = Value(
                "c",
                f"(64'hffffffffffffffff >> (64-({width})))",
                width=32 if bounded else 64,
            )
        return self.binary("AND", value, m)

    def const_text(self, expr):
        k = kind(expr)
        if k == "ConversionExpression":
            return self.const_text(expr.operand)
        if k == "IntegerLiteral":
            return str(int(expr.value))
        if k == "NamedValueExpression" and expr.symbol.kind.name == "Parameter":
            return expr.symbol.name
        if k == "BinaryExpression":
            op = {
                "Add": "+",
                "Subtract": "-",
                "Multiply": "*",
                "Divide": "/",
                "Mod": "%",
                "LogicalShiftLeft": "<<",
                "LogicalShiftRight": ">>",
            }.get(expr.op.name)
            if op:
                return (
                    "("
                    + self.const_text(expr.left)
                    + op
                    + self.const_text(expr.right)
                    + ")"
                )
        raise ValueError("Expected parameter expression: " + str(expr.syntax))

    def width(self, expr):
        if kind(expr) == "RangeSelectExpression":
            text = (
                "("
                + self.const_text(expr.left)
                + "-"
                + self.const_text(expr.right)
                + "+1)"
            )
            return text if re.search("[A-Z_]", text) else expr.type.bitWidth
        if kind(expr) == "ConversionExpression" and not expr.isImplicit and expr.syntax:
            text = str(expr.syntax).strip().split("'", 1)[0]
            if re.fullmatch(r"\d+", text):
                return int(text)
            if re.fullmatch(r"[A-Z_]+", text):
                return text
        return expr.type.bitWidth

    def io_read(self, name, width=64):
        if name == "packet_ready":
            return constant(1)
        if name not in self.io_cache:
            out = self.fresh()
            out = Value(out.kind, out.data, width=width)
            self.emit("IOREAD", out, imm=str(IO_ID[name]))
            self.io_cache[name] = out
        return self.io_cache[name]

    def expr(self, e, env):
        k = kind(e)
        if k == "IntegerLiteral":
            return constant(int(e.value))
        if k == "NamedValueExpression":
            symbol = e.symbol
            if key(symbol) in env:
                return env[key(symbol)]
            if symbol.kind.name == "Parameter":
                if symbol.name in self.states:
                    return Value("label", self.prefix + symbol.name, width=16)
                return Value("c", symbol.name, width=min(64, symbol.type.bitWidth))
            if symbol.name in self.external_values:
                return self.external_values[symbol.name](self)
            if symbol.name in self.command_values:
                return self.command_values[symbol.name]
            if symbol.name in self.inputs or symbol.name == "packet_ready":
                return self.io_read(symbol.name, min(64, symbol.type.bitWidth))
            if symbol.kind.name == "Net":
                raise ValueError("Unmapped kernel input " + symbol.name)
            if symbol.type.bitWidth > 64:
                raise ValueError("Wide value needs selection: " + symbol.name)
            return self.allocate(symbol)
        if k == "ConversionExpression":
            return self.mask(self.expr(e.operand, env), self.width(e))
        if k == "BinaryExpression":
            op = e.op.name
            a = self.expr(e.left, env)
            b = self.expr(e.right, env)
            if op in ["LogicalAnd", "LogicalOr"]:
                a = self.unary("BOOL", a)
                b = self.unary("BOOL", b)
                return self.binary("AND" if op == "LogicalAnd" else "OR", a, b)
            opcode = {
                "Add": "ADD",
                "Subtract": "SUB",
                "Multiply": "MUL32",
                "Divide": "DIV",
                "Mod": "MOD",
                "BinaryAnd": "AND",
                "BinaryOr": "OR",
                "BinaryXor": "XOR",
                "LogicalShiftLeft": "SHL",
                "LogicalShiftRight": "SHR",
                "ArithmeticShiftRight": "SHR",
                "Equality": "EQ",
                "Inequality": "NE",
                "LessThan": "LT",
                "LessThanEqual": "LE",
                "GreaterThan": "GT",
                "GreaterThanEqual": "GE",
            }.get(op)
            if not opcode:
                raise ValueError("Unsupported binary " + op)
            value = self.binary(opcode, a, b)
            return self.mask(value, self.width(e))
        if k == "UnaryExpression":
            opcode = {"LogicalNot": "NOT", "BitwiseNot": "INV", "UnaryPlus": "MOV"}.get(
                e.op.name
            )
            if not opcode:
                raise ValueError("Unsupported unary " + e.op.name)
            return self.mask(
                self.unary(opcode, self.expr(e.operand, env)), self.width(e)
            )
        if k == "RangeSelectExpression":
            value = self.expr(e.value, env)
            return self.mask(
                self.binary("SHR", value, self.expr(e.right, env)), self.width(e)
            )
        if k == "ElementSelectExpression":
            selector = self.expr(e.selector, env)
            if e.value.type.isUnpackedArray:
                base = self.allocate(e.value.symbol)
                if e.value.symbol.name == "c" and not self.content:
                    out = self.fresh()
                    out = Value(out.kind, out.data, width=32)
                    self.emit("CLOAD", out, self.reg(selector))
                    return out
                address = self.binary("ADD", Value("c", base.data), selector)
                out = self.fresh()
                out = Value(out.kind, out.data, width=e.type.bitWidth)
                self.emit("LOAD", out, self.reg(address))
                return out
            if e.value.type.bitWidth > 64:
                base = self.allocate(e.value.symbol)
                word = self.binary("SHR", selector, constant(6))
                address = self.binary("ADD", Value("c", base.data), word)
                value = self.fresh()
                self.emit("LOAD", value, self.reg(address))
                selector = self.binary("AND", selector, constant(63))
            else:
                value = self.expr(e.value, env)
            return self.mask(self.binary("SHR", value, selector), 1)
        if k == "ConcatenationExpression":
            value = constant(0)
            for operand in e.operands:
                width = self.width(operand)
                value = self.binary(
                    "SHL",
                    value,
                    constant(width) if isinstance(width, int) else Value("c", width),
                )
                value = self.binary("OR", value, self.expr(operand, env))
            return value
        if k == "ConditionalExpression":
            condition = self.expr(e.conditions[0].expr, env)
            out = self.fresh()
            otherwise = self.label()
            end = self.label()
            base = self.temp
            self.emit("JZERO", a=self.reg(condition), target=otherwise)
            self.emit("MOV", out, self.reg(self.expr(e.left, env)))
            self.emit("JUMP", target=end)
            self.mark(otherwise)
            self.temp = base
            self.emit("MOV", out, self.reg(self.expr(e.right, env)))
            self.mark(end)
            return out
        if k == "CallExpression":
            name = e.subroutineName
            args = [self.expr(x, env) for x in e.arguments]
            if name in ["packet_error", "first_set_bit"]:
                out = self.fresh()
                opcode = {
                    "packet_error": "PKTERR",
                    "first_set_bit": "FIRSTBIT",
                }[name]
                self.emit(
                    opcode,
                    out,
                    self.reg(args[0]),
                    self.reg(args[1]) if len(args) > 1 else None,
                )
                return out
            f = e.subroutine
            local = dict(env)
            for symbol, arg in zip(f.arguments, args):
                local[key(symbol)] = arg
            for symbol in f:
                if symbol.kind.name == "Variable":
                    local[key(symbol)] = self.fresh()
            self.blocking(f.body, local)
            return local[key(f.returnValVar)]
        raise ValueError("Unsupported expression " + k + " " + str(e.syntax))

    def location(self, e, env):
        k = kind(e)
        if k == "NamedValueExpression":
            symbol = e.symbol
            if symbol.name == self.pc_name:
                return ("pc", None, None, None)
            if symbol.name == "head_write":
                return ("head", None, None, None)
            if symbol.name in self.inputs:
                return ("io", symbol.name, None, None)
            target = env[key(symbol)] if key(symbol) in env else self.allocate(symbol)
            return ("reg", target, None, None)
        if k == "ElementSelectExpression":
            selector = self.expr(e.selector, env)
            if e.value.type.isUnpackedArray:
                base = self.allocate(e.value.symbol)
                if e.value.symbol.name == "c" and not self.content:
                    return ("context", self.reg(selector), None, None)
                address = self.reg(self.binary("ADD", Value("c", base.data), selector))
                return ("ind", address, None, None)
            if e.value.type.bitWidth > 64:
                base = self.allocate(e.value.symbol)
                address = self.reg(
                    self.binary(
                        "ADD",
                        Value("c", base.data),
                        self.binary("SHR", selector, constant(6)),
                    )
                )
                return ("ind", address, self.binary("AND", selector, constant(63)), 1)
            typ, target, _, _ = self.location(e.value, env)
            return (typ, target, selector, 1)
        if k == "RangeSelectExpression":
            typ, target, _, _ = self.location(e.value, env)
            return (typ, target, self.expr(e.right, env), self.width(e))
        raise ValueError("Unsupported lvalue " + k)

    def write(self, loc, value):
        typ, target, shift, width = loc
        if shift is not None:
            if typ == "reg":
                old = target
            elif typ in ["ind", "context"]:
                old = self.fresh()
                self.emit("CLOAD" if typ == "context" else "LOAD", old, target)
            else:
                raise ValueError("Partial I/O write")
            mask = (
                constant((1 << width) - 1)
                if isinstance(width, int)
                else Value("c", f"(64'hffffffffffffffff >> (64-({width})))")
            )
            shifted = self.binary("SHL", mask, shift)
            value = self.binary(
                "OR",
                self.binary("AND", old, self.unary("INV", shifted)),
                self.binary("SHL", self.binary("AND", value, mask), shift),
            )
        if typ == "reg" and value.kind in ["c", "label"]:
            imm = "@" + value.data if value.kind == "label" else f"32'({value.data})"
            self.emit("MOVI", target, imm=imm)
            if value.kind == "c" and (value.number is None or value.number >> 32):
                self.emit("MOVHI", target, a=target, imm=f"32'(({value.data}) >> 32)")
            value = target
        else:
            value = self.reg(value)
            if typ == "reg":
                self.emit("MOV", target, value)
        if typ == "reg":
            name = next(
                (name for ident, _, name in self.banks if ident == target.data), None
            )
            if name in self.output_alias:
                self.emit("IOWRITE", a=value, imm=str(IO_ID[self.output_alias[name]]))
        elif typ in ["ind", "context"]:
            self.emit("CSTORE" if typ == "context" else "STORE", a=target, b=value)
        elif typ == "io":
            # Input captures and kernel results are owned by the handshake bridge.
            if target not in ["op", "a0", "a1", "a2", "a3", "a4"]:
                self.emit("IOWRITE", a=value, imm=str(IO_ID[target]))
        else:
            raise ValueError("Unexpected write " + typ)

    def flatten(self, stmt):
        k = kind(stmt)
        if k == "TimedStatement":
            return self.flatten(stmt.stmt)
        if k == "BlockStatement":
            return self.flatten(stmt.body)
        if k == "StatementList":
            return list(stmt.list)
        return [stmt]

    def blocking(self, stmt, env):
        # Function-local control flow; functions never schedule nonblocking writes.
        for s in self.flatten(stmt):
            k = kind(s)
            if k == "VariableDeclStatement":
                continue
            if k in ["BlockStatement", "StatementList"]:
                self.blocking(s, env)
                continue
            if k == "ExpressionStatement":
                e = s.expr
                assert kind(e) == "AssignmentExpression" and not e.isNonBlocking
                self.write(self.location(e.left, env), self.expr(e.right, env))
            elif k == "ConditionalStatement":
                cond = self.expr(s.conditions[0].expr, env)
                other = self.label()
                end = self.label()
                base = self.temp
                self.emit("JZERO", a=self.reg(cond), target=other)
                self.blocking(s.ifTrue, env)
                self.emit("JUMP", target=end)
                self.mark(other)
                self.temp = base
                if s.ifFalse:
                    self.blocking(s.ifFalse, env)
                self.mark(end)
            else:
                raise ValueError("Unsupported function statement " + k)

    def finish(self, pending, next_, head):
        for loc, value in pending:
            self.write(loc, value)
        if head:
            self.emit("HEADWRITE")
        if next_.kind == "label":
            self.emit("JUMP", target=next_.data)
        else:
            self.emit("JIND", a=self.reg(next_))

    def sequence(self, statements, pending, env, next_, head=False):
        if not statements:
            self.finish(pending, next_, head)
            return
        stmt, *rest = statements
        k = kind(stmt)
        if k in ["BlockStatement", "StatementList", "TimedStatement"]:
            self.sequence(self.flatten(stmt) + rest, pending, env, next_, head)
            return
        if k == "VariableDeclStatement":
            self.sequence(rest, pending, env, next_, head)
            return
        if k == "ExpressionStatement":
            e = stmt.expr
            if kind(e) == "CallExpression" and e.subroutineKind.name == "Task":
                task = e.subroutine
                local = dict(env)
                args = [self.expr(x, env) for x in e.arguments]
                for symbol, arg in zip(task.arguments, args):
                    local[key(symbol)] = arg
                self.sequence(
                    self.flatten(task.body) + rest, pending, local, next_, head
                )
                return
            if kind(e) != "AssignmentExpression":
                raise ValueError("Unsupported expression statement " + str(e.syntax))
            loc = self.location(e.left, env)
            if loc[0] == "io" and loc[1] in ["op", "a0", "a1", "a2", "a3", "a4"]:
                self.sequence(rest, pending, env, next_, head)
                return
            if (
                kind(e.left) == "NamedValueExpression"
                and e.left.symbol.type.bitWidth > 64
            ):
                assert int(e.right.constant.value) == 0
                base = self.allocate(e.left.symbol)
                for i in range((e.left.symbol.type.bitWidth + 63) // 64):
                    pending = pending + [
                        (
                            ("reg", Value("r", base.data + f"+{i}"), None, None),
                            constant(0),
                        )
                    ]
                self.sequence(rest, pending, env, next_, head)
                return
            value = self.expr(e.right, env)
            if e.isNonBlocking:
                if loc[0] == "pc":
                    next_ = value
                elif loc[0] == "head":
                    head = value.number != 0
                else:
                    # Freeze RHS and dynamic LHS before any scheduled write commits.
                    if value.kind != "r" or value.data.startswith("R_TEMP+"):
                        saved = value
                    else:
                        saved = self.fresh()
                        self.emit("MOV", saved, value)
                    pending = pending + [(loc, saved)]
            else:
                self.write(loc, value)
            self.sequence(rest, pending, env, next_, head)
            return
        if k == "ConditionalStatement":
            cond = self.expr(stmt.conditions[0].expr, env)
            other = self.label()
            base = self.temp
            cache = dict(self.io_cache)
            self.emit("JZERO", a=self.reg(cond), target=other)
            self.sequence(
                self.flatten(stmt.ifTrue) + rest, list(pending), dict(env), next_, head
            )
            self.mark(other)
            self.temp = base
            self.io_cache = dict(cache)
            self.sequence(
                (self.flatten(stmt.ifFalse) if stmt.ifFalse else []) + rest,
                list(pending),
                dict(env),
                next_,
                head,
            )
            return
        if k == "CaseStatement":
            value = self.reg(self.expr(stmt.expr, env))
            base = self.temp
            cache = dict(self.io_cache)
            for group in stmt.items:
                self.temp = base
                self.io_cache = dict(cache)
                test = constant(0)
                for e in group.expressions:
                    test = self.binary(
                        "OR", test, self.binary("EQ", value, self.expr(e, env))
                    )
                following = self.label()
                self.emit("JZERO", a=self.reg(test), target=following)
                branchbase = self.temp
                self.sequence(
                    self.flatten(group.stmt) + rest,
                    list(pending),
                    dict(env),
                    next_,
                    head,
                )
                self.mark(following)
                self.temp = base
            self.sequence(
                (self.flatten(stmt.defaultCase) if stmt.defaultCase else []) + rest,
                list(pending),
                dict(env),
                next_,
                head,
            )
            return
        if k == "ForLoopStatement":
            stop = stmt.stopExpr.right
            while kind(stop) == "ConversionExpression":
                stop = stop.operand
            assert (
                stmt.stopExpr.op.name == "LessThan"
            ), "Only ascending zero-based clear loops are supported"
            variable = stmt.loopVars[0]
            assert (
                int(variable.initializer.constant.value) == 0
            ), "Clear loops must start at zero"
            step = stmt.steps[0]
            assert kind(step) == "AssignmentExpression" and key(
                step.left.symbol
            ) == key(variable)
            assert (
                step.right.op.name == "Add"
                and int(step.right.right.constant.value) == 1
            )
            assert key(stmt.stopExpr.left.symbol) == key(variable)
            # Parameter-sized loops only clear independent complete array words.
            for sub in self.flatten(stmt.body):
                e = sub.expr
                assert kind(e) == "AssignmentExpression" and e.isNonBlocking
                assert (
                    kind(e.left) == "ElementSelectExpression"
                    and e.left.value.type.isUnpackedArray
                )
                assert int(e.right.constant.value) == 0
                base = self.allocate(e.left.value.symbol)
                self.emit(
                    "FILL",
                    a=self.reg(Value("c", base.data)),
                    b=self.reg(self.expr(stop, env)),
                )
            self.sequence(rest, pending, env, next_, head)
            return
        raise ValueError("Unsupported statement " + k + " " + str(stmt.syntax))

    def compile(self):
        if self.content:
            return self.compile_content()
        skip = {
            "READ_WAIT",
            "WRITE_WAIT",
            "STORE_WAIT",
            "CONTEXT_READ_NEXT",
            "CONTEXT_WRITE_NEXT",
            "HISTORY_READ_NEXT",
            "HISTORY_WRITE_NEXT",
            "COPY_WRITE",
            "COPY_NEXT",
            "ZERO_NEXT",
            "JOB_SELECT",
            "JOB_SELECT_COMPARE",
            "BOOT_CONTEXT",
            "BOOT_CONTEXT_NEXT",
        }
        for name, body in self.states.items():
            if name in skip:
                continue
            self.temp = 0
            self.io_cache = {}
            self.mark(name)
            if name == "IDLE":
                self.emit("COMMAND")
                assert kind(body) == "ConditionalStatement"
                body = body.ifTrue
            elif name == "RESPONSE":
                self.emit("REPLY")
                self.emit("JUMP", target="IDLE")
                continue
            elif name == "JOB_SELECT_BEGIN":
                self.emit("MERGE")
                self.write(
                    ("reg", self.variable("scan"), None, None),
                    Value("c", "LOOPS", width=32),
                )
                self.emit("JZERO", a=self.variable("found"), target="RESPONSE")
                self.write(
                    ("reg", self.variable("context_slot"), None, None),
                    self.variable("best_slot"),
                )
                self.write(("reg", self.variable("cursor"), None, None), constant(0))
                self.write(
                    ("reg", self.variable("context_return"), None, None),
                    Value("label", "PROCESS_EMIT", width=16),
                )
                self.emit("JUMP", target="CONTEXT_READ")
                continue
            elif name == "BOOT_CONTEXTS":
                self.emit("CTXZERO")
                self.write(("reg", self.variable("scan"), None, None), constant(0))
                self.emit("JUMP", target="BOOT_HISTORIES")
                continue
            elif name in ["HISTORY_READ", "HISTORY_WRITE"]:
                f = next(n for n in self.body if n.name == "history_address")
                env = {}
                env[key(f.arguments[0])] = self.variable("input_slot")
                for symbol in f:
                    if symbol.kind.name == "Variable":
                        env[key(symbol)] = self.fresh()
                self.blocking(f.body, env)
                address = env[key(f.returnValVar)]
                base = self.reg(Value("c", self.variable("h").data))
                self.emit(
                    "BULKIN" if name == "HISTORY_READ" else "BULKOUT",
                    rd=self.reg(constant(24)),
                    a=address if name == "HISTORY_READ" else base,
                    b=base if name == "HISTORY_READ" else address,
                    target="memory_fault",
                )
                self.write(("reg", self.variable("cursor"), None, None), constant(23))
                self.emit("JIND", a=self.variable("history_return"))
                continue
            elif name in ["COPY_READ", "ZERO_WRITE"]:
                self.emit(
                    "BULKCOPY" if name == "COPY_READ" else "BULKZERO",
                    rd=self.variable("copy_count"),
                    a=self.variable("copy_source"),
                    b=self.variable("copy_target"),
                    target="memory_fault",
                )
                self.write(
                    ("reg", self.variable("copy_index"), None, None),
                    self.variable("copy_count"),
                )
                self.emit("JIND", a=self.variable("copy_return"))
                continue
            elif name in ["CONTEXT_READ", "CONTEXT_WRITE"]:
                self.emit(
                    "CTXREAD" if name == "CONTEXT_READ" else "CTXWRITE",
                    a=self.variable("context_slot"),
                )
                self.write(("reg", self.variable("cursor"), None, None), constant(31))
                self.emit("JIND", a=self.variable("context_return"))
                continue
            elif name == "STORE_START" and self.other_body is not None:
                self.emit("JUMP", target="S_IDLE")
                continue
            elif name in [
                "READ_REQUEST",
                "WRITE_REQUEST",
                "STORE_START",
            ]:
                opcode = {
                    "READ_REQUEST": "MEMREAD",
                    "WRITE_REQUEST": "MEMWRITE",
                    "STORE_START": "CONTENT",
                }[name]
                self.emit(opcode, target="memory_fault")
                ret = (
                    "memory_return"
                    if name in ["READ_REQUEST", "WRITE_REQUEST"]
                    else "service_return"
                )
                symbol = next(s for s in self.body if s.name == ret)
                self.emit("JIND", a=self.allocate(symbol))
                continue
            elif name == "EMIT_PACKET":
                self.emit("PACKET")
            elif name == "JOB_SELECT":
                symbol = next(s for s in self.body if s.name == "scan")
                self.emit("HEADREAD", a=self.allocate(symbol))
            self.sequence(self.flatten(body), [], {}, Value("label", name))
        self.temp = 0
        self.io_cache = {}
        self.mark("memory_fault")
        task = next(s for s in self.body if s.name == "memory_fault")
        self.sequence(self.flatten(task.body), [], {}, Value("label", "RESPONSE"))
        return self

    def write_files(self, check=False):
        out = ROOT / "rtl/midi_control_program.svh"
        lines = [
            "// Generated by compile_control.py.",
            "localparam integer MIDI_UCODE_WORDS = " + str(len(self.code)) + ";",
            "localparam integer MIDI_UCODE_BITS = $clog2(MIDI_UCODE_WORDS);",
            "localparam integer MIDI_UCODE_DEPTH = 1 << MIDI_UCODE_BITS;",
        ]
        lines.append(f"localparam integer MIDI_CONST_WORDS = {len(self.constants)};")
        for _, (i, value) in self.constants.items():
            lines.append(f"localparam integer R_CONST_{i} = {i+1};")
        offset = "1+MIDI_CONST_WORDS"
        for ident, size, name in self.banks:
            lines.append(f"localparam integer {ident} = {offset};")
            offset = f"{ident}+{size}"
        lines += [
            f"localparam integer R_TEMP = {offset};",
            f"localparam integer MIDI_WORK_WORDS = R_TEMP+{self.peak_temp};",
        ]
        for label, pc in self.labels.items():
            lines.append(f"localparam [15:0] U_{label} = 16'd{pc};")
        lines += [f"localparam [7:0] MC_{name} = 8'd{i};" for name, i in OP.items()]
        lines += [f"localparam [7:0] MI_{name} = 8'd{i};" for name, i in IO_ID.items()]
        save_generated(out, "\n".join(lines) + "\n", check)
        words = []
        for pc, i in enumerate(self.code):
            op = f'MC_{i["op"]}'
            rd = f"10'({i['rd']})"
            a = f"10'({i['a']})"
            b = f"10'({i['b']})"
            if i["op"] in ["MOVI", "MOVHI"]:
                imm = i["imm"]
                imm = f"U_{imm[1:]}" if imm.startswith("@") else imm
                word = f"{{32'({imm}),14'b0,{rd},{op}}}"
            elif i["op"] in ["IOREAD", "IOWRITE"]:
                word = f"{{16'd{i['imm']},20'b0,{a},{rd},{op}}}"
            elif i["target"] is not None:
                word = f"{{U_{i['target']},10'b0,{b},{a},{rd},{op}}}"
            else:
                word = f"{{26'b0,{b},{a},{rd},{op}}}"
            words.append(word)
        bank_words = 2048
        bank_count = (len(words) + bank_words - 1) // bank_words
        rom = [
            "// Generated by compile_control.py.",
            "localparam integer MIDI_ROM_BANK_BITS=11;",
            "localparam integer MIDI_ROM_SELECT_BITS=MIDI_UCODE_BITS>11 ? MIDI_UCODE_BITS-11 : 1;",
            "reg [MIDI_ROM_SELECT_BITS-1:0] code_bank;",
            "always @(posedge clk) if(phase==FETCH) code_bank<=MIDI_ROM_SELECT_BITS'(micro_pc>>11);",
        ]
        for bank in range(bank_count):
            name = f"control_bank_{bank}"
            rom += [
                f'(* ram_style="block", rom_style="block" *) reg [63:0] {name}[0:2047];',
                f"reg [63:0] {name}_q;",
                "initial begin",
                f"    for(integer i=0;i<2048;i=i+1) {name}[i]=0;",
            ]
            for index, word in enumerate(
                words[bank * bank_words : (bank + 1) * bank_words]
            ):
                rom.append(f"    {name}[{index}]={word};")
            rom += [
                "end",
                f"always @(posedge clk) if(phase==FETCH && MIDI_ROM_SELECT_BITS'(micro_pc>>11)==MIDI_ROM_SELECT_BITS'({bank}))",
                f"    {name}_q<={name}[micro_pc[10:0]];",
            ]
        rom += ["always @* begin", "    case(code_bank)"]
        for bank in range(bank_count):
            rom.append(
                f"        MIDI_ROM_SELECT_BITS'({bank}): fetched_instruction=control_bank_{bank}_q;"
            )
        rom += ["        default: fetched_instruction=0;", "    endcase", "end"]
        save_generated(ROOT / "rtl/midi_control_rom.svh", "\n".join(rom) + "\n", check)
        save_generated(
            ROOT / "program/control-layout.json",
            json.dumps(
                {
                    "instructions": len(self.code),
                    "temporary_words": self.peak_temp,
                    "constant_words": len(self.constants),
                    "banks": self.banks,
                    "labels": self.labels,
                    "io": IO_ID,
                },
                indent=2,
            )
            + "\n",
            check,
        )
        print(
            f"{len(self.code)} instructions, {self.peak_temp} temporary words, {len(self.banks)} working banks"
        )

    def variable(self, name):
        return self.allocate(next(s for s in self.body if s.name == name))

    def compile_content(self):
        for name, body in self.states.items():
            if name in ["READ_WAIT", "WRITE_WAIT"]:
                continue
            self.temp = 0
            self.io_cache = {}
            self.mark(self.prefix + name)
            if name == "IDLE":
                assert kind(body) == "ConditionalStatement"
                body = body.ifTrue
            elif name == "RESPONSE":
                fields = {
                    "last_error": "response_error",
                    "last_handle": "response_handle",
                    "last_frame": "response_frame",
                    "last_order": "response_order",
                    "last_packet": "response_packet",
                    "last_length": "response_length",
                }
                for target, source in fields.items():
                    if target == "last_error":
                        self.emit(
                            "IOWRITE", a=self.variable(source), imm=str(IO_ID[target])
                        )
                    else:
                        self.write(
                            ("reg", self.other_body.variable(target), None, None),
                            self.variable(source),
                        )
                desc = self.variable("desc")
                for target, index in [
                    ("last_events", 5),
                    ("last_status", 2),
                    ("last_blocks", 3),
                    ("last_owner", 15),
                    ("last_references", 1),
                ]:
                    self.write(
                        ("reg", self.other_body.variable(target), None, None),
                        Value("r", desc.data + "+" + str(index)),
                    )
                error = self.binary("NE", self.variable("response_error"), constant(7))
                self.emit("JZERO", a=self.reg(error), target="memory_fault")
                self.emit("JIND", a=self.other_body.variable("service_return"))
                continue
            elif name in ["READ_REQ", "WRITE_REQ"]:
                self.emit(
                    "MEMREAD" if name == "READ_REQ" else "MEMWRITE",
                    target="S_memory_fault",
                )
                self.emit("JIND", a=self.variable("return_phase"))
                continue
            self.sequence(
                self.flatten(body), [], {}, Value("label", self.prefix + name)
            )
        self.temp = 0
        self.io_cache = {}
        self.mark("S_memory_fault")
        self.write(("reg", self.variable("formatted"), None, None), constant(0))
        self.write(("reg", self.variable("response_error"), None, None), constant(7))
        self.emit("JUMP", target="S_RESPONSE")
        return self


def combined():
    control = Compiler()
    content = Compiler(content=True)
    control.other_body = content
    content.other_body = control
    content.constants = control.constants
    for name, source in {
        "command_op": "store_op",
        "command_view": "store_view",
        "command_arg0": "store_arg0",
        "command_arg1": "store_arg1",
        "command_packet": "store_packet",
        "command_length": "store_length",
    }.items():
        content.command_values[name] = control.variable(source)
    for name, field in [("free_blocks", "free_count"), ("store_work", "response_work")]:
        control.external_values[name] = lambda c, field=field: content.variable(field)
    for name, field, capacity in [
        ("free_views", "used_views", "VIEWS"),
        ("free_retirements", "queue_count", "RETIREMENTS"),
    ]:
        control.external_values[name] = (
            lambda c, field=field, capacity=capacity: c.binary(
                "SUB", Value("c", capacity), content.variable(field)
            )
        )
    control.compile()
    content.compile()
    offset = len(control.code)
    control.code += content.code
    control.labels.update(
        {name: address + offset for name, address in content.labels.items()}
    )
    control.banks += content.banks
    control.peak_temp = max(control.peak_temp, content.peak_temp)
    control.mark("BOOT")
    for _, (i, value) in control.constants.items():
        target = Value("r", "R_CONST_" + str(i))
        control.emit("MOVI", target, imm=f"32'({value.data})")
        if value.number is None or value.number >> 32:
            control.emit("MOVHI", target, a=target, imm=f"32'(({value.data}) >> 32)")
    control.emit("JUMP", target="IDLE")
    for instruction in control.code:
        if instruction["target"] is not None:
            assert instruction["target"] in control.labels, instruction
    return control


def save_generated(path, text, check):
    if check:
        if not path.is_file() or path.read_text() != text:
            raise ValueError("Stale generated file: " + str(path))
    else:
        path.write_text(text)
