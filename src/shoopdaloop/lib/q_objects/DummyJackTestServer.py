from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQuick import QQuickItem

from ..logging import Logger as BaseLogger

import random
import string
import subprocess
import time
import signal
import ctypes

import jacklib
from jacklib.helpers import *

libc = ctypes.CDLL("libc.so.6")
def set_pdeathsig(sig = signal.SIGTERM):
    def callable():
        return libc.prctl(1, sig)
    return callable

def generate_random_string(length):
    letters = string.ascii_lowercase
    return ''.join(random.choice(letters) for i in range(length))

def open_client(logger, name, server_name='default'):
    logger.info("Opening test client: {}".format(name))
    status = jacklib.jack_status_t()
    client = jacklib.client_open(name, jacklib.JackServerName, status, server_name)
    if status.value:
        logger.error("Failed to open client {}: {}, {}".format(name, status.value, get_jack_status_error_string(status)))
        raise Exception()
    return client

class Client():
    def __init__(self, logger, name, server_name):
        self.ports = dict()
        self.name = name
        self.logger = logger
        try:
            self.client = open_client(logger, name, server_name)
            jacklib.activate(self.client)
        except:
            self.client = None
    
    def open_port(self, name, is_audio, is_input):
        self.logger.info("Opening test port: {}:{}".format(self.name, name))
        port_direction = jacklib.JackPortIsInput if is_input else jacklib.JackPortIsOutput
        port_type = jacklib.JACK_DEFAULT_AUDIO_TYPE if is_audio else jacklib.JACK_DEFAULT_MIDI_TYPE
        self.ports[name] = (jacklib.port_register(self.client, name, port_type, port_direction, 0))
    
    def close_port(self, name):
        self.logger.info("Closing test port: {}:{}".format(self.name, name))
        if name in self.ports.keys():
            jacklib.port_unregister(self.client, self.ports[name])
            del self.ports[name]

    def close(self):
        if self.client:
            jacklib.client_close(self.client)
            self.client = None

    def __del__(self):
        self.close()

class DummyJackTestServer(QQuickItem):
    def __init__(self, parent=None):
        super(DummyJackTestServer, self).__init__(parent)
        self.logger = BaseLogger("Frontend.DummyJackTestServer")
        self._server_name = generate_random_string(10)
        self.jackd = None
        self.clients = dict()

        result = subprocess.run('which jackd', shell=True, capture_output=True)
        if result.returncode != 0:
            self.logger.error("Jackd not found.\nstdout:\n{}\nstderr:\n{}".format(result.stdout.decode(), result.stderr.decode()))
            raise Exception("Jackd not found")
    
        self.jackd = subprocess.Popen('jackd -T -r -n {} -d dummy -C 0 -P 0'.format(self._server_name), shell=True, preexec_fn = set_pdeathsig(signal.SIGTERM))

        time.sleep(1.0)

        if self.jackd.poll() != None:
            self.logger.error("Jackd failed to start.")
            raise Exception("Jackd failed to start")
        
        self.logger.info("Jackd started with server name {}".format(self._server_name))
    
    ###########
    ## SLOTS
    ###########

    @Slot()
    def delete_all_test_clients(self):
        for client in self.clients.values():
            client.close()
        self.clients = dict()
        time.sleep(0.5)

    @Slot(str, str, bool, bool)
    def ensure_test_port(self, client_name, port_name, is_audio, is_input):
        if not client_name in self.clients.keys():
            self.clients[client_name] = Client(self.logger, client_name, self._server_name)
            time.sleep(0.2)
        if not port_name in self.clients[client_name].ports.keys():
            self.clients[client_name].open_port(port_name, is_audio, is_input)
            time.sleep(0.2)
    
    @Slot(str, str)
    def close_test_port(self, client_name, port_name):
        if client_name in self.clients.keys():
            self.clients[client_name].close_port(port_name)
            time.sleep(0.2)

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