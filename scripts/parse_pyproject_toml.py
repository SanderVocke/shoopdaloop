#!/usr/bin/python

# Usage: parse_pyproject_toml.py file.toml my::nested::key::i::want::to::print

import toml
import sys

c = toml.load(sys.argv[1])

key = sys.argv[2]
for part in key.split('::'):
    c = c[part]

print(c)
