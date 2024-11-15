#!/usr/bin/python

import glob
import sys
import os
import shutil

arg_idx = 1
directory = False
separator = None
executable = False
do_copy = None

for arg in sys.argv[arg_idx:]:
    if arg == 'directory':
        directory = True
        arg_idx += 1
    elif arg == 'executable':
        executable = True
        arg_idx += 1
    elif arg == 'separator':
        arg_idx += 1
        separator = sys.argv[arg_idx]
        arg_idx += 1
    elif arg == 'do_copy':
        arg_idx += 1
        do_copy = sys.argv[arg_idx]
        arg_idx += 1
    else:
        break

results = glob.glob(sys.argv[arg_idx], recursive=True)
if directory:
    results = [(os.path.dirname(r) if os.path.isfile(r) else r) for r in results]
if executable:
    results = [r for r in results if os.path.isfile(r) and os.access(r, os.X_OK)]

results = list(set([os.path.realpath(r) for r in results]))

if do_copy:
    if len(results) == 0:
        print("ERROR: no results to copy")
        exit(1)
    for r in results:
        if not os.path.isfile(r):
            print("ERROR: cannot copy as it is not a file: " + r)
            exit(1)
        shutil.copyfile(r, do_copy)

if separator:
    print(separator.join(results))
else:
    for r in results:
        print(r)