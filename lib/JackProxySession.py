import subprocess
import os
import time

import sys
sys.path.append('..')

from third_party.pyjacklib import jacklib

class JackProxySession:
    def __init__(self, server_name, n_capture, n_playback):
        self.server_name = server_name

        script_pwd = os.path.dirname(__file__)
        jackd_path = script_pwd + '/../build/jack2/jackd'
        jack_so_path = script_pwd + '/../build/jack2'
        jack_client_lib_path = script_pwd + '/../build/jack2/client/libjack.so.0'

        env = os.environ.copy()
        env["JACK_DRIVER_DIR"] = jack_so_path
        env["LD_LIBRARY_PATH"] = jack_so_path
        
        cmd = '{} -n {} -d proxy -C {} -P {}'.format(jackd_path, server_name, n_capture, n_playback)
        print("Running jackd proxy.\n  Command: {}\n".format(cmd))
        self.proc = subprocess.Popen(cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            shell=True,
            env=env)

        self.jacklib = jacklib.load_libjack(jack_client_lib_path)
        
        # Give it time to become available
        time.sleep(3.0)
    
    def __enter__(self):
        # Return a jacklib instance which can be used for executing commands in the session.
        return self.jacklib

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
        
    def wait(self, timeout = None):
        try:
            self.proc.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            self.proc.terminate()
            self.proc.wait()