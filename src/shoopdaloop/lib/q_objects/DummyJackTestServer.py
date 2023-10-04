from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQuick import QQuickItem

from ..logging import Logger as BaseLogger

import random
import string
import subprocess
import time
import signal
import ctypes

libc = ctypes.CDLL("libc.so.6")
def set_pdeathsig(sig = signal.SIGTERM):
    def callable():
        return libc.prctl(1, sig)
    return callable

def generate_random_string(length):
    letters = string.ascii_lowercase
    return ''.join(random.choice(letters) for i in range(length))

class DummyJackTestServer(QQuickItem):
    def __init__(self, parent=None):
        super(DummyJackTestServer, self).__init__(parent)
        self.logger = BaseLogger("Frontend.DummyJackTestServer")
        self._server_name = generate_random_string(10)
        self.jackd = None

        result = subprocess.run('which jackd', shell=True, capture_output=True)
        if result.returncode != 0:
            self.logger.error("Jackd not found.\nstdout:\n{}\nstderr:\n{}".format(result.stdout.decode(), result.stderr.decode()))
            raise Exception("Jackd not found")
    
        self.jackd = subprocess.Popen('jackd --no-realtime -n {} -d dummy'.format(self._server_name), shell=True, preexec_fn = set_pdeathsig(signal.SIGTERM))

        time.sleep(1.0)

        if self.jackd.poll() != None:
            self.logger.error("Jackd failed to start.")
            raise Exception("Jackd failed to start")
        
        self.logger.info("Jackd started with server name {}".format(self._server_name))
    
    ###########
    ## SLOTS
    ###########

    @Slot()
    def close(self):
        self.logger.info("Closing server")
        time.sleep(1.0)
        if self.jackd:
            self.jackd.kill()
            self.jackd = None
            self.logger.info("Jackd terminated")

    @Slot(result=str)
    def get_server_name(self):
        return self._server_name
    
    def __del__(self):
        self.close()