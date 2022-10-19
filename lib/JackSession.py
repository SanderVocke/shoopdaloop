import subprocess
import os
import time
import signal
import threading

import sys
sys.path.append('..')

from third_party.pyjacklib import jacklib
from lib.Colorcodes import Colorcodes

class JackSession:
    def __init__(self, client_name):
        self.client_name = client_name

        script_pwd = os.path.dirname(__file__)
        self.jlib = jacklib.JacklibInstance('libjack.so.0')

    def __enter__(self):
        # Try to create a client repeatedly for a specified amount of time.
        status = jacklib.jack_status_t()
        start_t = time.monotonic()
        while time.monotonic() - start_t < 5.0:
            time.sleep(0.2)
            self.jack_client = self.jlib.client_open(self.client_name, jacklib.JackNoStartServer, status)
            #self.jack_client = jacklib.client_open(self.client_name, jacklib.JackNoStartServer, status)
            if self.jack_client and status.value == 0:
                break
        if not self.jack_client or status.value != 0:
            raise Exception("Failed to start client in jack session: status {}".format(status.value))
        
        return [self.jlib, self.jack_client]

    def __exit__(self, type, value, traceback):
        pass