from ctypes import *
from functools import partial

import sys
sys.path.append('..')

import build.backend.shoopdaloop_backend as backend

class BackendSession:
    def __init__(self,
                 n_loops,
                 loops_per_track, 
                 n_channels,
                 client_name):
        self.update_cb = backend.UpdateCallback(self.update_cb);
        self.abort_cb = backend.AbortCallback(self.abort_cb)

        loops_to_ports = (c_uint * n_loops)()
        input_port_names = (POINTER(c_char) * n_loops)()
        output_port_names = (POINTER(c_char) * n_loops)()
        
        for l in range(n_loops):
            loops_to_ports[l] = l
            input_port_names[l] = create_string_buffer('loop_{}_in'.format(l+1).encode('ascii'))
            output_port_names[l] = create_string_buffer('loop_{}_out'.format(l+1).encode('ascii'))
        
        backend.initialize(
            n_loops,
            n_loops,
            480000,
            2,
            loops_to_ports,
            input_port_names,
            output_port_names,
            client_name.encode('ascii'),
            100,
            self.update_cb,
            self.abort_cb
        )
    
    def __enter__(self):
        return self

    def __exit__(self, type, value, traceback):
        pass

    def update_cb(
                self,
                n_loops,
                n_ports,
                loop_states,
                loop_lengths,
                loop_positions,
                loop_volumes,
                port_volumes,
                port_passthrough_levels):
        return 0

    def abort_cb(self):
        print("Backend aborted.")
        exit(1)