from PySide6.QtCore import QMetaObject, Qt, Q_ARG
from shoop_rust import *
from shiboken6 import getCppPointer

def transition_gui_loop(loop, mode, maybe_delay, maybe_align_to_sync_at):
    if hasattr(loop, "is_composite_loop"):
        loop.transition(mode, maybe_delay, maybe_align_to_sync_at)
    else:
        shoop_rust_transition_gui_loop(
            getCppPointer(loop)[0],
            mode,
            maybe_delay,
            maybe_align_to_sync_at
        )

def transition_gui_loops(loops, mode, maybe_delay, maybe_align_to_sync_at):
    py_loops = [l for l in loops if hasattr(l, "is_composite_loop")]
    backend_loops = [l for l in loops if not hasattr(l, "is_composite_loop")]
    if len(backend_loops) > 0:
        shoop_rust_transition_gui_loops(
            [getCppPointer(loop)[0] for loop in backend_loops],
            mode,
            maybe_delay,
            maybe_align_to_sync_at
        )
    for l in py_loops:
        l.transition(mode, maybe_delay, maybe_align_to_sync_at)

def transition_backend_loop(loop, mode, maybe_delay, maybe_align_to_sync_at):
    if hasattr(loop, "is_composite_loop"):
        loop.transition(mode, maybe_delay, maybe_align_to_sync_at)
    else:
        shoop_rust_transition_backend_loop(
            getCppPointer(loop)[0],
            mode,
            maybe_delay,
            maybe_align_to_sync_at
        )

def transition_backend_loops(loops, mode, maybe_delay, maybe_align_to_sync_at):
    py_loops = [l for l in loops if hasattr(l, "is_composite_loop")]
    backend_loops = [l for l in loops if not hasattr(l, "is_composite_loop")]
    if len(backend_loops) > 0:
        shoop_rust_transition_backend_loops(
            [getCppPointer(loop)[0] for loop in backend_loops],
            mode,
            maybe_delay,
            maybe_align_to_sync_at
        )
    for l in py_loops:
        l.transition(mode, maybe_delay, maybe_align_to_sync_at)

def gui_loop_adopt_ringbuffers(loop, reverse_start, n_cycles, go_to_cycle, go_to_mode):
    if hasattr(loop, "is_composite_loop"):
        loop.adopt_ringbuffers(reverse_start, n_cycles, go_to_cycle, go_to_mode)
    else:
        shoop_rust_gui_loop_adopt_ringbuffers(
            getCppPointer(loop)[0],
            reverse_start,
            n_cycles,
            go_to_cycle,
            go_to_mode
        )

def backend_loop_adopt_ringbuffers(loop, reverse_start, n_cycles, go_to_cycle, go_to_mode):
    if hasattr(loop, "is_composite_loop"):
        loop.adopt_ringbuffers(reverse_start, n_cycles, go_to_cycle, go_to_mode)
    else:
        shoop_rust_backend_loop_adopt_ringbuffers(
            getCppPointer(loop)[0],
            reverse_start,
            n_cycles,
            go_to_cycle,
            go_to_mode
        )