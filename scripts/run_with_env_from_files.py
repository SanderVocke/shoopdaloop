#!/usr/bin/python

# Helper script useful in Cmake when a build step needs an env setting based on
# the output of another env step.

# Usage: run_with_env_from_files.py env1=file1 env2=file2 -- command
# Will run the command with the env variables indicated set to the values loaded
# from the given files.

import sys
import subprocess
import os

env = dict()
ori_env = os.environ
first_idx = 2
for arg in sys.argv[1:]:
    if arg == '--':
        break
    with open(arg.split('=')[1], 'r') as f:
        env[arg.split('=')[0]] = f.read().strip()
    first_idx += 1

print("Executing with env {}: {}".format(env, sys.argv[first_idx:]))

ori_env.update(env)
exit(subprocess.run(sys.argv[first_idx:], env=ori_env).returncode)