import subprocess
import os
import time
import signal
import threading

import sys
sys.path.append('..')

from third_party.pyjacklib import jacklib

class JackProxySession:
    def __init__(self, server_name, n_capture, n_playback, client_name):
        self.server_name = server_name
        self.client_name = client_name

        script_pwd = os.path.dirname(__file__)
        jackd_path = script_pwd + '/../build/jack2/jackd'
        jack_so_path = script_pwd + '/../build/jack2'
        jack_client_lib_path = script_pwd + '/../build/jack2/client/libjack.so.0'

        env = os.environ.copy()
        env["JACK_DRIVER_DIR"] = jack_so_path
        env["LD_LIBRARY_PATH"] = jack_so_path
        
        cmd = '{} -d proxy -C {} -P {}'.format(jackd_path, n_capture, n_playback)
        if self.server_name:
            cmd = '{} -n {} -d proxy -C {} -P {}'.format(jackd_path, server_name, n_capture, n_playback)
        print("Running jackd proxy.\n  Command: {}\n".format(cmd))
        self.proc = subprocess.Popen(cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            shell=True,
            env=env)
        def print_lines():
            for line in self.proc.stdout:
                if line:
                    print('jackd: ' + line.decode('utf8'), end='')
        self.print_thread = threading.Thread(target=print_lines)
        self.print_thread.start()

        jacklib.jlib = jacklib.load_libjack(jack_client_lib_path)
    
    def __enter__(self):
        # Try to create a client repeatedly for a specified amount of time.
        status = jacklib.jack_status_t()
        start_t = time.monotonic()
        while time.monotonic() - start_t < 5.0:
            if self.server_name:
                self.jack_client = jacklib.client_open(self.client_name, jacklib.JackNoStartServer | jacklib.JackServerName, status, self.server_name)
            else:
                self.jack_client = jacklib.client_open(self.client_name, jacklib.JackNoStartServer, status)
            if status.value == 0:
                break
        if status.value != 0:
            raise Exception("Failed to start client in jack proxy session: status {}".format(status.value))
        return self.jack_client

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