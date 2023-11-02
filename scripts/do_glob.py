#!/usr/bin/python

import glob
import sys

results = glob.glob(sys.argv[1], recursive=True)

for r in results:
    print(r)