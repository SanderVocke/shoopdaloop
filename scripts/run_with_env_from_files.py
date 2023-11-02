#!/usr/bin/python
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