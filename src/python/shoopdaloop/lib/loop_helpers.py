from PySide6.QtCore import QMetaObject, Qt, Q_ARG
from shoop_rust import shoop_rust_transition_loop, shoop_rust_transition_loops
from shiboken6 import getCppPointer

def transition_loop(loop, mode, maybe_delay, maybe_align_to_sync_at):
    shoop_rust_transition_loop(
        getCppPointer(loop)[0],
        mode,
        maybe_delay,
        maybe_align_to_sync_at
    )

def transition_loops(loops, mode, maybe_delay, maybe_align_to_sync_at):
    shoop_rust_transition_loops(
        [getCppPointer(loop)[0] for loop in loops],
        mode,
        maybe_delay,
        maybe_align_to_sync_at
    )