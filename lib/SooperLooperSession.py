import subprocess
import os
import time
import signal
import threading

import sys
sys.path.append('..')

from third_party.pyjacklib import jacklib

class SooperLooperSession:
    def __init__(self, n_loops, n_channels, port, jack_server_name, client_name, ld_library_path):
        env = os.environ.copy()
        env["LD_LIBRARY_PATH"] = ld_library_path

        cmd = 'sooperlooper -l {} -c {} -p {} -j {} -S {}'.format(n_loops, n_channels, port, client_name, jack_server_name if jack_server_name else 'default')
        print("Running sooperlooper.\n  Command: {}\n  LD_LIBRARY_PATH: {}\n".format(cmd, ld_library_path))
        self.proc = subprocess.Popen(cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            shell=True,
            env=env)
        def print_lines():
            for line in self.proc.stdout:
                if line:
                    print('sooperlooper: ' + line.decode('utf8'), end='')
        self.print_thread = threading.Thread(target=print_lines)
        self.print_thread.start()
        
        # Give it time to become available
        time.sleep(3.0)
    
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