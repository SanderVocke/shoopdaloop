from ctypes import *

from PyQt6.QtCore import QObject, pyqtSignal, pyqtSlot, QTimer

import sys
sys.path.append('../..')

import build.backend.shoopdaloop_backend as backend
from lib.LoopState import *
from collections import OrderedDict

from pprint import *

class BackendManager(QObject):
    update = pyqtSignal(
        int,  # n loops
        int,  # n ports
        list, # states
        list, # next_states
        list, # lengths
        list, # positions
        list, # loop volumes
        list, # port volumes
        list  # port passthrough levels
    )

    def __init__(self,
                 port_names_to_loop_amounts,
                 channels_per_loop,
                 soft_sync_master_loop,
                 max_loop_length_seconds,
                 client_name,
                 update_period_seconds,
                 parent=None):
        '''
        Initialize the backend manager.

        Parameters:
            port_names_to_loop_amounts (dict):
                An OrderedDict of (input_name,output_name) tuples to the amount of loops to assign to that port pair.
            channels_per_loop (int):
                Amount of channels to assign to each loop
            client_name (str):
                Name of the JACK client to instantiate
        '''
        super(BackendManager, self).__init__(parent)

        self.update_cb = backend.UpdateCallback(self.update_cb)
        self.abort_cb = backend.AbortCallback(self.abort_cb)

        self.n_loops = sum(port_names_to_loop_amounts.values()) * channels_per_loop
        self.n_ports = len(port_names_to_loop_amounts.items())
        self.n_channels = channels_per_loop
        self.client_name = client_name
        self.port_names_to_loop_amounts = port_names_to_loop_amounts
        self.soft_sync_master_loop = soft_sync_master_loop
        self.max_loop_length_seconds = max_loop_length_seconds

        self.initialize_backend()

        self.update_timer = QTimer()
        self.update_timer.setSingleShot(False)
        self.update_timer.setInterval(int(1000.0 * update_period_seconds))
        self.update_timer.timeout.connect(lambda: backend.request_update())
        self.update_timer.start()
    
    def initialize_backend(self):
        loops_to_ports = (c_uint * self.n_loops)()
        loops_hard_sync_map = (c_int * self.n_loops)()
        loops_soft_sync_map = (c_int * self.n_loops)()
        input_port_names = (POINTER(c_char) * self.n_ports)()
        output_port_names = (POINTER(c_char) * self.n_ports)()

        # Construct the port names and loop mappings
        loop_idx = 0
        port_idx = 0
        for i in range(self.n_loops):
            loops_hard_sync_map[i] = i
            loops_soft_sync_map[i] = self.soft_sync_master_loop
        
        for item in self.port_names_to_loop_amounts.items():
            input_port_name = item[0][0]
            output_port_name = item[0][1]
            N = item[1]

            input_port_names[port_idx] = create_string_buffer(input_port_name.encode('ascii'))
            output_port_names[port_idx] = create_string_buffer(output_port_name.encode('ascii'))

            for i in range(self.n_channels):
                loops_to_ports[loop_idx] = port_idx
                if i > 0:
                    # Should be hard-synced to the first channel
                    loops_hard_sync_map[loop_idx] = loop_idx - i
                loop_idx += 1

            port_idx += 1
        
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
            self.abort_cb
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
        self.update.emit(
            int(n_loops),
            int(n_ports),
            [int(loop_states[i]) for i in range(n_loops)],
            [int(loop_next_states[i]) for i in range(n_loops)],
            [int(loop_lengths[i]) for i in range(n_loops)],
            [int(loop_positions[i]) for i in range(n_loops)],
            [float(loop_volumes[i]) for i in range(n_loops)],
            [float(port_volumes[i]) for i in range(n_ports)],
            [float(port_passthrough_levels[i]) for i in range(n_ports)],
        )

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
            c_uint(loop_idx),
            backend.loop_action_t(action_id)
        )
