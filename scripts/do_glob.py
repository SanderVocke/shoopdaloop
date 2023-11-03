#!/usr/bin/python

import glob
import sys
import os

arg_idx = 1
directory = False
separator = None

for arg in sys.argv[arg_idx:]:
    if arg == 'directory':
        directory = True
        arg_idx += 1
    elif arg == 'separator':
        arg_idx += 1
        separator = sys.argv[arg_idx]
        arg_idx += 1
    else:
        break

results = glob.glob(sys.argv[arg_idx], recursive=True)
if directory:
    results = [(os.path.dirname(r) if os.path.isfile(r) else r) for r in results]

if separator:
    print(separator.join(results))
else:
    for r in results:
        print(r)