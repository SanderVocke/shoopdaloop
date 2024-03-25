#!/usr/bin/python

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

        bare_args = []
        current_args = []
        for a in sys.argv[1:]:
            if a == '--':
                bare_args.append(current_args)
                current_args = []
            else:
                current_args.append(a)   
        bare_args.append(current_args)             

        parser = argparse.ArgumentParser(
            prog="ShoopDaLoop",
            description="An Audio+MIDI looper with DAW features"
        )
        parser.add_argument('session_filename', type=str, default=None, nargs='?', help='(optional) Load a session from a file upon startup.')
        parser.add_argument('-i', '--info', action='store_true', help='Show information about the ShoopDaLoop installation.')
        parser.add_argument('-b', '--backend', type=str, default='jack', help='Choose an audio backend to use. Available backends (default = jack): {}'.format(', '.join([b.name.lower() for b in AudioDriverType])))

        dev_group = parser.add_argument_group('Developer options')
        dev_group.add_argument('-m', '--main', type=str, default='shoopdaloop_main', help='Choose a specific app main window to open. Any choice other than the default is usually for debugging. Available windows: {}'.format(', '.join(mains)))
        dev_group.add_argument('-d', '--qml-debug', metavar='PORT', type=int, help='Start QML debugging on PORT')
        dev_group.add_argument('-w', '--debug-wait', action='store_true', help='With QML debugging enabled, will wait until debugger connects.')
        dev_group.add_argument('-e', '--developer', action='store_true', help='Enable developer functionality in the UI.')
        dev_group.add_argument('--test-grab-screens', type=str, help='For debugging: will open several windows, grab and save screenshots of them, store them in the given folder (will create if not existing) and then exit.')
        dev_group.add_argument('--quit-after', type=float, default=-1.0, help='For debugging: quit X seconds after app is fully loaded.')
        dev_group.add_argument('--monkey-tester', action='store_true', help='Start the monkey tester, which will randomly, rapidly perform actions on the session.')
        dev_group.add_argument('--qml-self-test', action='store_true', help='Run QML tests and exit. Pass additional args to the tester after "--".')
        
        args = parser.parse_args(bare_args[0])
        
        from shoopdaloop.lib.q_objects.Application import Application
        from shoopdaloop.lib.crash_handling import init_crash_handling
        init_crash_handling()

        backends_map = {b.name.lower(): b for b in AudioDriverType}
        args.backend = backends_map[args.backend]

        # For taskbar icon (see https://stackoverflow.com/a/1552105)
        if platform.system() == 'Windows':
            myappid = 'shoopdaloop.shoopdaloop' # arbitrary string
            ctypes.windll.shell32.SetCurrentProcessExplicitAppUserModelID(myappid)
            
        if args.qml_self_test:
            import shoopdaloop.qml_tests
            result = shoopdaloop.qml_tests.run_qml_tests(bare_args[1] if len(bare_args) > 1 else [])
            sys.exit(result)

        if args.info:
            version=None
            with open(installation_dir() + '/version.txt', 'r') as f:
                version = f.read()
            print('ShoopDaLoop {}'.format(version.strip()))
            print('Installed @ {}'.format(installation_dir()))
            print('Scripts @ {}'.format(scripts_dir()))
            sys.exit(0)
        
        global_args = {
            'backend_type': args.backend.value,
            'load_session_on_startup': args.session_filename,
            'test_grab_screens': args.test_grab_screens,
            'developer': args.developer,
            'quit_after': args.quit_after,
            'monkey_tester': bool(args.monkey_tester)
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
        sys.exit(1)


if __name__ == "__main__":
    main()