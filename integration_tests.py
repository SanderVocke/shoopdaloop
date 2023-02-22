#!/bin/python

import sys

from lib.StatesAndActions import *
from lib.JackSession import JackSession
from third_party.pyjacklib import jacklib
from lib.q_objects.BackendManager import BackendManager

from collections import OrderedDict

from lib.port_loop_mappings import get_port_loop_mappings

import signal
import psutil
import os
import time
import random
import string
import unittest
import contextlib

# Ensure that we forward any terminating signals to our child
# processes
def exit_handler(sig, frame):
    print('Got signal {}.'.format(sig))
    current_process = psutil.Process()
    children = current_process.children(recursive=False)
    for child in children:
        print('Send signal {} => {}'.format(sig, child.pid))
        os.kill(child.pid, sig)
    print('Exiting.')
    sys.exit(0)

signal.signal(signal.SIGINT, exit_handler)
signal.signal(signal.SIGQUIT, exit_handler)
signal.signal(signal.SIGTERM, exit_handler)

script_pwd = os.path.dirname(__file__)

def is_running(pid):        
    try:
        os.kill(pid, 0)
    except OSError as err:
        if err.errno == errno.ESRCH:
            return False
    return True

class BackendTests(unittest.TestCase):

    def setUp(self):
        print("\n\nTESTCASE START")
        self.stack = contextlib.ExitStack()
        self.stack.__enter__()
        self.session = JackSession('test-backend')
        self.session_ctx = self.stack.enter_context(self.session)
        self.jack = self.session_ctx[0]
        self.jack_client = self.session_ctx[1]

        self.mappings = get_port_loop_mappings(
            8,
            6,
            ['l', 'r']
        )

        self.backend_mgr = BackendManager(
            self.mappings['port_name_pairs'],
            self.mappings['loops_to_ports'],
            self.mappings['loops_hard_sync'],
            self.mappings['loops_sync'],
            60.0,
            'test-backend',
            0.03 # About 30Hz updates
        )
        self.backend_mgr_ctx = self.stack.enter_context(self.backend_mgr)
    
    def tearDown(self):
        self.stack.__exit__(None, None, None)
    
    def test_basic(self):
        pass

    def test_loop_data_roundtrip(self):
        # Skip every second loop because they are hard-linked
        for i in [j*2 for j in range(int(self.backend_mgr.n_loops/2))]:
            data = [float(j) for j in range(i)] # Some loop-dependent data
            self.backend_mgr.load_loop_data(i, data)
            self.assertListEqual(self.backend_mgr.get_loop_data(i), data)

if __name__ == '__main__':
    unittest.main()