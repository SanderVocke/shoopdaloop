#!/usr/bin/python

import glob
import sys
import os

arg = sys.argv[1]
directory = False
if arg == 'directory':
    directory = True
    arg = sys.argv[2]

results = glob.glob(sys.argv[1], recursive=True)

for r in results:
    if os.path.isfile(r) and directory:
        print(os.path.dirname(r))
    else:
        print(r)