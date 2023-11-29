import sys
import os

script_pwd = os.path.dirname(__file__)

import shoopdaloop.lib
module_pwd = os.path.dirname(shoopdaloop.lib.__file__) + '/..' 

import traceback
import argparse
import glob
import ctypes
import platform

from shoopdaloop.lib.q_objects.Application import Application
from shoopdaloop.lib.logging import *
from shoopdaloop.lib.backend_wrappers import *

def main():
    logger = Logger("Frontend.Main")    
    try:
        mains = [ os.path.basename(p).replace('.qml','') for p in glob.glob('{}/lib/qml/applications/*.qml'.format(script_pwd)) ]

        parser = argparse.ArgumentParser(
            prog="ShoopDaLoop",
            description="An Audio+MIDI looper with some DAW-like features"
        )
        parser.add_argument('-d', '--qml-debug', metavar='PORT', type=int, help='Start QML debugging on PORT')
        parser.add_argument('-w', '--debug-wait', action='store_true', help='With QML debugging enabled, will wait until debugger connects.')
        parser.add_argument('-i', '--info', action='store_true', help='Show information about the ShoopDaLoop installation.')
        parser.add_argument('-m', '--main', type=str, default='shoopdaloop_main', help='Choose a specific app main window to open. Any choice other than the default is usually for debugging. Available windows: {}'.format(', '.join(mains)))
        parser.add_argument('-b', '--backend', type=str, default='jack', help='Choose an audio backend to use. Available backends (default = jack): {}'.format(', '.join([b.name.lower() for b in BackendType])))
        parser.add_argument('-j', '--jack-server', type=str, default='default', help='Choose a JACK server to connect to. (default = default)')
        parser.add_argument('--test-grab-screens', type=str, help='For debugging: will open several windows, grab and save screenshots of them, store them in the given folder (will create if not existing) and then exit.')
        parser.add_argument('session_filename', type=str, default=None, nargs='?', help='(optional) Load a session from a file upon startup.')
        args = parser.parse_args()

        backends_map = {b.name.lower(): b for b in BackendType}
        args.backend = backends_map[args.backend]

        # For taskbar icon (see https://stackoverflow.com/a/1552105)
        if platform.system() == 'Windows':
            myappid = 'shoopdaloop.shoopdaloop' # arbitrary string
            ctypes.windll.shell32.SetCurrentProcessExplicitAppUserModelID(myappid)

        if args.info:
            version=None
            with open(script_pwd + '/version.txt', 'r') as f:
                version = f.read()
            print('ShoopDaLoop {}'.format(version.strip()))
            print('Installed @ {}'.format(script_pwd))
            exit(0)
        
        global_args = {
            'backend_type': args.backend.value,
            'backend_argstring': args.jack_server if args.backend == BackendType.Jack else '',
            'load_session_on_startup': args.session_filename,
            'test_grab_screens': args.test_grab_screens
        }
    
        app = Application(
            'ShoopDaLoop',
            '{}/lib/qml/applications/{}.qml'.format(module_pwd, args.main),
            global_args,
            dict(),
            args.qml_debug,
            args.debug_wait,
            True
            )
        app.exec()
    except Exception as e:
        logger.error(lambda: "Exception: " + str(e) + "\n" + traceback.format_exc())
        exit()


if __name__ == "__main__":
    main()