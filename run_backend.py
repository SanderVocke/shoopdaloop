import sys

from lib.q_objects.BackendManager import BackendManager
from lib.LoopState import *
from lib.JackSession import JackSession
from third_party.pyjacklib import jacklib

from collections import OrderedDict

from lib.port_loop_mappings import get_port_loop_mappings

import signal
import psutil
import os
import time
import random
import string

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

with JackSession('test-sdl-backend') as jack_session:
    jack = jack_session[0]
    jack_client = jack_session[1]

    mappings = get_port_loop_mappings(
        8,
        6,
        ['l', 'r']
    )
    
    # Start the back-end
    with BackendManager(
        mappings['port_name_pairs'],
        mappings['loops_to_ports'],
        mappings['loops_hard_sync'],
        mappings['loops_soft_sync'],
        60.0,
        'test-sdl-backend',
        0.03 # About 30Hz updates
    ) as backend_mgr:

        for arg in sys.argv[1:]:
            match arg:
                case 'play':
                    for i in range(len(mappings['loops_to_ports'])):
                        backend_mgr.do_loop_action(i, LoopActionType.DoPlay.value, [])
                case 'stop':
                    for i in range(len(mappings['loops_to_ports'])):
                        backend_mgr.do_loop_action(i, LoopActionType.DoStop.value, [])
                case 'record':
                    for i in range(len(mappings['loops_to_ports'])):
                        backend_mgr.do_loop_action(i, LoopActionType.DoRecord.value, [])

        while(True):
            pass
