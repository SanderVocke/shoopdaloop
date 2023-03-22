#!/bin/python

import sys
import os
import traceback
script_dir = os.path.dirname(os.path.realpath(__file__))
sys.path.append(script_dir + '/..')

from lib.q_objects.Application import Application
app = Application('ShoopDaLoop',
    '{}/../lib/qml/applications/shoopdaloop_main.qml'.format(script_dir),
    sys.argv)
exitcode = app.exec()