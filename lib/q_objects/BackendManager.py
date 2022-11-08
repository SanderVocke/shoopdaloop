from ctypes import *

from PyQt6.QtCore import QObject, pyqtSignal, pyqtSlot, QTimer

from .NChannelAbstractLooperManager import NChannelAbstractLooperManager

import sys
sys.path.append('../..')

import build.backend.shoopdaloop_backend as backend
from lib.LoopState import *
from collections import OrderedDict

import time
import cProfile

from pprint import *

class BackendManager(QObject):
    def __init__(self,
                 port_name_pairs,
                 loops_to_ports_map,
                 loops_hard_sync_map,
                 loops_soft_sync_map,
                 max_loop_length_seconds,
                 client_name,
                 update_period_seconds,
                 parent=None):
        super(BackendManager, self).__init__(parent)

        self.update_cb = backend.UpdateCallback(self.update_cb)
        self.abort_cb = backend.AbortCallback(self.abort_cb)

        self.n_loops = len(loops_to_ports_map)
        self.n_ports = len(port_name_pairs)
        self.client_name = client_name
        self.max_loop_length_seconds = max_loop_length_seconds
        self.port_name_pairs = port_name_pairs
        self.loops_to_ports = loops_to_ports_map
        self.loops_soft_sync_map = loops_soft_sync_map
        self.loops_hard_sync_map = loops_hard_sync_map

        self.update_timer = QTimer()
        self.update_timer.setSingleShot(False)
        self.update_timer.setInterval(int(1000.0 * update_period_seconds))
        self.update_timer.timeout.connect(lambda: backend.request_update())
        self.update_timer.start()

        self.looper_mgrs = [
            NChannelAbstractLooperManager(self) for i in range(self.n_loops)
        ]
    
    def __enter__(self):
        self.initialize_backend()
        return self

    def __exit__(self, type, value, traceback):
        backend.terminate()
    
    def initialize_backend(self):
        loops_to_ports = (c_uint * self.n_loops)()
        loops_hard_sync_map = (c_int * self.n_loops)()
        loops_soft_sync_map = (c_int * self.n_loops)()
        input_port_names = (POINTER(c_char) * self.n_ports)()
        output_port_names = (POINTER(c_char) * self.n_ports)()

        # Construct the port names and loop mappings        
        for port_idx, item in enumerate(self.port_name_pairs):
            input_port_names[port_idx] = create_string_buffer(item[0].encode('ascii'))
            output_port_names[port_idx] = create_string_buffer(item[1].encode('ascii'))
        
        for i in range(self.n_loops):
            loops_to_ports[i] = self.loops_to_ports[i]
            loops_hard_sync_map[i] = self.loops_hard_sync_map[i]
            loops_soft_sync_map[i] = self.loops_soft_sync_map[i]
        
        backend.initialize(
            self.n_loops,
            self.n_ports,
            self.max_loop_length_seconds,
            loops_to_ports,
            loops_hard_sync_map,
            loops_soft_sync_map,
            input_port_names,
            output_port_names,
            self.client_name.encode('ascii'),
            1,
            self.update_cb,
            self.abort_cb,
            backend.backend_features_t(backend.Default)
        )

    def update_cb(
                self,
                n_loops,
                n_ports,
                loop_states,
                loop_next_states,
                loop_lengths,
                loop_positions,
                loop_volumes,
                port_volumes,
                port_passthrough_levels):
        # pr = cProfile.Profile()
        # pr.enable()
        for i in range(n_loops):
            m = self.looper_mgrs[i]
            m.state = loop_states[i]
            m.next_state = loop_next_states[i]
            m.length = loop_lengths[i]
            m.pos = loop_positions[i]
            m.volume = loop_volumes[i]
        # # TODO port changes
        # pr.disable()
        # pr.print_stats()
        return 0

    def abort_cb(self):
        print("Backend aborted.")
        exit(1)
    
    @pyqtSlot(int, int, list)
    def do_loop_action(self, loop_idx, action_id, args):
        if loop_idx < 0 or loop_idx >= self.n_loops:
            raise ValueError("Backend: loop idx out of range")
        
        if action_id < 0 or \
           action_id >= backend.LOOP_ACTION_MAX:
           raise ValueError("Backend: loop action {} is not implemented in back-end".format(action_id))
        
        backend.do_loop_action(
            byref(c_uint(loop_idx)),
            1,
            backend.loop_action_t(action_id)
        )
    
    @pyqtSlot(int, list)
    def load_loop_data(self, loop_idx, data):
        c_data = (c_float * len(data))(*data)
        backend.load_loop_data(loop_idx, len(data), c_data)
