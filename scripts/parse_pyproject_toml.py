#!/usr/bin/python

import toml
import sys

c = toml.load(sys.argv[1])

key = sys.argv[2]
for part in key.split('::'):
    c = c[part]

print(c)
