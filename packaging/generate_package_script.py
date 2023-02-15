#!/bin/python

import sys
import os
import glob

scriptdir = os.path.dirname(os.path.realpath(__file__))

args = sys.argv[1:]

template_filename = args[0]
replacement_args = args[1:]

replacements = dict()
for r in replacement_args:
    rr = r.split('=', maxsplit=1)
    replacements[rr[0]] = rr[1]

template = ''
with open(template_filename, 'r') as f:
    template = f.read()

for (t, r) in replacements.items():
    template = template.replace('***{}***'.format(t), r)

if template.find('***') != -1:
    print('Error: not all template arguments were replaced', file=sys.stderr)
    exit(1)

print(template)