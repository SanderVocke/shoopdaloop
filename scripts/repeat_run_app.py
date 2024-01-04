#!/usr/bin/python

# Script for repeatedly running an application and seeing if it
# fails, closes unexpectedly or remains unexpectedly open.

import argparse
import signal
import time
import subprocess
import platform

parser = argparse.ArgumentParser()
parser.add_argument('-i', '--iterations', type=int, default=1, help='Amount of iterations to run')
parser.add_argument('-w', '--wait-ms-before-close', type=int, default=100, help='How long to wait before closing a run')
parser.add_argument('-k', '--wait-ms-before-kill', type=int, default=2000, help='How long to wait before killing a run')
parser.add_argument('-o', '--expect-stay-open', type=int, default=0, help='Whether to expect the app to stay open until closed')
parser.add_argument('command', nargs=argparse.REMAINDER, help='Command to run')
args = parser.parse_args()

n_unexpected_open = 0
n_unexpected_closed = 0
n_had_to_kill = 0
n_nonzero_exit = 0

def terminate(process):
    process.terminate()

def exit_code_ok(code):
    if platform.system() == 'Windows':
        return code in [0, 1, 0xC000013A]
    else:
        return code == 0

for i in range(args.iterations):
    print('''
=====================================================
== REPEAT_RUN_APP: RUN {} of {}
=====================================================       
'''.format(i+1, args.iterations))
    
    def wait_and_poll(process, timeout):
        tstart = time.time()
        while process.poll() is None and (time.time() - tstart) < (timeout / 1000):
            time.sleep(0.05)
        return process.poll()
    
    # Start the process
    process = subprocess.Popen(args.command)

    # Wait for args.wait_ms_before_close milliseconds
    wait_and_poll(process, args.wait_ms_before_close)
    
    # Check if the process is still running unexpectedly
    if process.poll() is None and not args.expect_stay_open:
        print('== REPEAT_RUN_APP ERROR: Process is still running unexpectedly.')
        n_unexpected_open += 1
    elif process.poll() is not None and args.expect_stay_open:
        print(args.expect_stay_open)
        print('== REPEAT_RUN_APP ERROR: Process exited unexpectedly.')
        n_unexpected_closed += 1
    
    # Try to terminate the process gracefully
    print('== REPEAT_RUN_APP: Terminate')
    terminate(process)
    if wait_and_poll(process, args.wait_ms_before_kill) is None:
        # Process is still running, so we'll use SIGKILL to force it to exit
        print('== REPEAT_RUN_APP ERROR: Process did not exit gracefully, hard-killing')
        process.kill()
        n_had_to_kill += 1
    else:
        # Process exited gracefully
        print('== REPEAT_RUN_APP: Process exited gracefully after termination')
    
    if process.poll() is not None and not exit_code_ok(process.poll()):
        print('== REPEAT_RUN_APP ERROR: Process exited with non-OK exit code {}'.format(process.poll()))
        n_nonzero_exit += 1

if n_unexpected_open == 0 and n_unexpected_closed == 0 and n_nonzero_exit == 0 and n_had_to_kill == 0:
    print('''
=====================================================
== REPEAT_RUN_APP: ALL OK
=====================================================       
''')
    exit(0)
else:
    print('''
=====================================================
== REPEAT_RUN_APP: FAILURE
== - Iterations: {}
== - App was unexpectedly open: {}
== - App was unexpectedly closed: {}
== - App had nonzero exit code: {}
== - Had to hard-kill: {}
=====================================================
'''.format(
    args.iterations,
    n_unexpected_open,
    n_unexpected_closed,
    n_nonzero_exit,
    n_had_to_kill
))
    exit(1)
    
    
    
    