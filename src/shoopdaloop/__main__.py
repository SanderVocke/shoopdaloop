import sys
import os

script_pwd = os.path.dirname(__file__)

import traceback
import argparse

from .lib.q_objects.Application import Application
from .lib.logging import *

def main():
    logger = Logger("Frontend.Main")    
    try:
        parser = argparse.ArgumentParser(
            prog="ShoopDaLoop",
            description="An Audio+MIDI looper with some DAW-like features"
        )
        parser.add_argument('-d', '--qml-debug', metavar='PORT', type=int, help='Start QML debugging on PORT')
        parser.add_argument('-w', '--debug-wait', action='store_true', help='With QML debugging enabled, will wait until debugger connects.')
        parser.add_argument('-i', '--info', action='store_true', help='Show information about the ShoopDaLoop installation.')
        args = parser.parse_args()

        if args.info:
            version=None
            with open(script_pwd + '/version.txt', 'r') as f:
                version = f.read()
            print('ShoopDaLoop {}'.format(version.strip()))
            print('Installed @ {}'.format(script_pwd))
            exit(0)
    
        app = Application('ShoopDaLoop',
            '{}/lib/qml/applications/shoopdaloop_main.qml'.format(script_pwd),
            args.qml_debug,
            args.debug_wait
            )
        app.exec()
    except Exception as e:
        logger.error("Exception: " + str(e) + "\n" + traceback.format_exc())
        exit()