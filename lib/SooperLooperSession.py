import subprocess
import os
import time
import signal
import threading

import sys
sys.path.append('..')

from third_party.pyjacklib import jacklib
from lib.Colorcodes import Colorcodes

class SooperLooperSession:
    def __init__(self, n_loops, loops_per_track, n_channels, port, client_name, jack, jack_client):
        self.colorchar = Colorcodes().green.decode('utf8')
        self.resetchar = Colorcodes().reset.decode('utf8')

        script_pwd = os.path.dirname(__file__)
        env = os.environ.copy()
        cmd = script_pwd + '/../build/sooperlooper -l {} -c {} -p {} -j {}'.format(n_loops, n_channels, port, client_name)
        print("Running sooperlooper.\n  Command: {}\n".format(cmd))
        self.proc = subprocess.Popen(cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            shell=True,
            env=env)
        def print_lines():
            for line in self.proc.stdout:
                if line:
                    print(self.colorchar + 'sooperlooper: ' + line.decode('utf8') + self.resetchar, end='')
        self.print_thread = threading.Thread(target=print_lines)
        self.print_thread.start()
    
    def __enter__(self):
        return self

    def __exit__(self, type, value, traceback):
        print('sooperlooper exiting.')
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