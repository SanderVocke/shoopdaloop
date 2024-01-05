import sys
import os
from shoopdaloop.lib.directories import *

import traceback
import argparse
import glob
import ctypes
import platform

def main():
    from shoopdaloop.lib.logging import Logger
    logger = Logger("Frontend.Main")    
    try:
        from shoopdaloop.lib.backend_wrappers import AudioDriverType
        mains = [ os.path.basename(p).replace('.qml','') for p in glob.glob('{}/lib/qml/applications/*.qml'.format(installation_dir())) ]

        parser = argparse.ArgumentParser(
            prog="ShoopDaLoop",
            description="An Audio+MIDI looper with some DAW-like features"
        )
        parser.add_argument('-d', '--qml-debug', metavar='PORT', type=int, help='Start QML debugging on PORT')
        parser.add_argument('-w', '--debug-wait', action='store_true', help='With QML debugging enabled, will wait until debugger connects.')
        parser.add_argument('-i', '--info', action='store_true', help='Show information about the ShoopDaLoop installation.')
        parser.add_argument('-m', '--main', type=str, default='shoopdaloop_main', help='Choose a specific app main window to open. Any choice other than the default is usually for debugging. Available windows: {}'.format(', '.join(mains)))
        parser.add_argument('-b', '--backend', type=str, default='jack', help='Choose an audio backend to use. Available backends (default = jack): {}'.format(', '.join([b.name.lower() for b in AudioDriverType])))
        parser.add_argument('-e', '--developer', action='store_true', help='Enable developer functionality.')
        parser.add_argument('--test-grab-screens', type=str, help='For debugging: will open several windows, grab and save screenshots of them, store them in the given folder (will create if not existing) and then exit.')
        parser.add_argument('--quit-when-loaded', action='store_true', help='For debugging: quit immediately when fully loaded.')
        parser.add_argument('session_filename', type=str, default=None, nargs='?', help='(optional) Load a session from a file upon startup.')
        args = parser.parse_args()
        
        from shoopdaloop.lib.q_objects.Application import Application
        from shoopdaloop.lib.crash_handling import init_crash_handling
        init_crash_handling()

        backends_map = {b.name.lower(): b for b in AudioDriverType}
        args.backend = backends_map[args.backend]

        # For taskbar icon (see https://stackoverflow.com/a/1552105)
        if platform.system() == 'Windows':
            myappid = 'shoopdaloop.shoopdaloop' # arbitrary string
            ctypes.windll.shell32.SetCurrentProcessExplicitAppUserModelID(myappid)

        if args.info:
            version=None
            with open(installation_dir() + '/version.txt', 'r') as f:
                version = f.read()
            print('ShoopDaLoop {}'.format(version.strip()))
            print('Installed @ {}'.format(installation_dir()))
            print('Scripts @ {}'.format(scripts_dir()))
            exit(0)
        
        global_args = {
            'backend_type': args.backend.value,
            'load_session_on_startup': args.session_filename,
            'test_grab_screens': args.test_grab_screens,
            'developer': args.developer,
            'quit_when_loaded': args.quit_when_loaded
        }
    
        app = Application(
            'ShoopDaLoop',
            '{}/lib/qml/applications/{}.qml'.format(scripts_dir(), args.main),
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