import subprocess
import os
import time
import signal
import threading

import sys
sys.path.append('..')

from third_party.pyjacklib import jacklib
from lib.Colorcodes import Colorcodes

class JackProxySession:
    def __init__(self, server_name, capture_port_names, playback_port_names, client_name):
        self.server_name = server_name
        self.client_name = client_name
        self.server_ready = False
        self.colorchar = Colorcodes().blue.decode('utf8')
        self.resetchar = Colorcodes().reset.decode('utf8')

        script_pwd = os.path.dirname(__file__)
        jackd_path = script_pwd + '/../build/jack2/jackd'
        jack_so_path = script_pwd + '/../build/jack2'
        jack_client_lib_path = script_pwd + '/../build/jack2/client/libjack.so.0'

        # Note: hacky to do this via the process environment, couldn't get it to
        # work in any other way
        os.environ['JACK_DEFAULT_SERVER'] = self.server_name

        env = os.environ.copy()
        env["JACK_DRIVER_DIR"] = jack_so_path
        env["LD_LIBRARY_PATH"] = jack_so_path

        captureports = ','.join(capture_port_names)
        playbackports = ','.join(playback_port_names)
        n_captureports = len(capture_port_names)
        n_playbackports = len(playback_port_names)
        env['JACK_PROXY_PLAYBACK_CHANNEL_NAMES'] = playbackports
        env['JACK_PROXY_CAPTURE_CHANNEL_NAMES'] = captureports
        
        cmd = '{} -d proxy -C {} -P {} -n {}'.format(jackd_path, n_captureports, n_playbackports, client_name)
        print("Running jackd proxy.\n  Command: {}. JACK_DEFAULT_SERVER: {}. LD_LIBRARY_PATH: {}.".format(cmd, env['JACK_DEFAULT_SERVER'], env['LD_LIBRARY_PATH']))
        self.proc = subprocess.Popen(cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            shell=True,
            env=env)
        def print_lines():
            for line in self.proc.stdout:
                if line:
                    print(self.colorchar + 'jackd: ' + line.decode('utf8') + self.resetchar, end='')
                    if 'JackProxyDriver connected to ' in line.decode('utf8'):
                        self.server_ready = True
        self.print_thread = threading.Thread(target=print_lines)
        self.print_thread.start()
        self.jlib = jacklib.JacklibInstance(jack_client_lib_path)

        start_t = time.monotonic()
        while time.monotonic() - start_t < 5.0 and not self.server_ready:
            time.sleep(0.1)
        if not self.server_ready:
            raise Exception("Jackd did not come online.")

    def __enter__(self):
        # Try to create a client repeatedly for a specified amount of time.
        status = jacklib.jack_status_t()
        start_t = time.monotonic()
        while time.monotonic() - start_t < 5.0:
            time.sleep(0.2)
            self.jack_client = self.jlib.client_open(self.client_name, jacklib.JackNoStartServer | jacklib.JackServerName, status, self.server_name)
            #self.jack_client = jacklib.client_open(self.client_name, jacklib.JackNoStartServer, status)
            if self.jack_client and status.value == 0:
                break
        if not self.jack_client or status.value != 0:
            raise Exception("Failed to start client in jack proxy session: status {}".format(status.value))
        
        return [self.jlib, self.jack_client]

    def __exit__(self, type, value, traceback):
        print('jackd exiting.')
        if self.proc and not self.proc.poll():
            try:
                os.killpg(self.proc.pid, signal.SIGTERM)
                self.proc.wait(timeout=5.0)
            except ProcessLookupError:
                self.proc.terminate()
            except subprocess.TimeoutExpired:
                self.proc.kill()
        self.wait(0.5)
        self.print_thread.join()

        
    def wait(self, timeout = None):
        try:
            self.proc.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            self.proc.terminate()
            self.proc.wait()