import sys
import os

import traceback
import argparse
import glob
import ctypes
import platform

from shoop_config import shoop_qml_dir, shoop_install_info, shoop_version

def main():
    from shoopdaloop.lib.logging import Logger
    logger = Logger("Frontend.Main")
    try:
        import shoop_py_backend
        mains = [ os.path.basename(p).replace('.qml','') for p in glob.glob('{}/applications/*.qml'.format(shoop_qml_dir)) ]

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
        parser.add_argument('-v', '--version', action='store_true', help='Show version information and exit.')
        parser.add_argument('-b', '--backend', type=str, default='jack', help='Choose an audio backend to use. Available backends (default = jack): {}'.format(', '.join([name.lower() for name, val in shoop_py_backend.AudioDriverType.enum_items().items()])))

        dev_group = parser.add_argument_group('Developer options')
        dev_group.add_argument('-m', '--main', type=str, default='shoopdaloop_main', help='Choose a specific app main window to open. Any choice other than the default is usually for debugging. Available windows: {}'.format(', '.join(mains)))
        dev_group.add_argument('-d', '--qml-debug', metavar='PORT', type=int, help='Start QML debugging on PORT')
        dev_group.add_argument('-w', '--debug-wait', action='store_true', help='With QML debugging enabled, will wait until debugger connects.')
        dev_group.add_argument('-e', '--developer', action='store_true', help='Enable developer functionality in the UI.')
        dev_group.add_argument('--test-grab-screens', type=str, help='For debugging: will open several windows, grab and save screenshots of them, store them in the given folder (will create if not existing) and then exit.')
        dev_group.add_argument('--quit-after', type=float, default=-1.0, help='For debugging: quit X seconds after app is fully loaded.')
        dev_group.add_argument('--monkey-tester', action='store_true', help='Start the monkey tester, which will randomly, rapidly perform actions on the session.')
        dev_group.add_argument('--qml-self-test', action='store_true', help='Run QML tests and exit. Pass additional args to the tester after "--".')

        from shoopdaloop.lib.q_objects.Application import Application

        args = parser.parse_args(bare_args[0])
        backends_map = {name.lower(): shoop_py_backend.AudioDriverType(val) for name, val in shoop_py_backend.AudioDriverType.enum_items().items()}
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
            print('ShoopDaLoop {}'.format(shoop_version))
            print('Installation: {}'.format(shoop_install_info))
            sys.exit(0)

        if args.version:
            print(shoop_version)
            sys.exit(0)

        global_args = {
            'backend_type': int(args.backend),
            'load_session_on_startup': args.session_filename,
            'test_grab_screens': args.test_grab_screens,
            'developer': args.developer,
            'quit_after': args.quit_after,
            'monkey_tester': bool(args.monkey_tester)
        }

        app = Application(
            'ShoopDaLoop',
            '{}/applications/{}.qml'.format(shoop_qml_dir, args.main),
            global_args,
            dict(),
            args.qml_debug,
            args.debug_wait,
            True
            )
        app.exec()

        return 0
    except SystemExit as e:
        return e.code
    except Exception as e:
        logger.error(lambda: "Exception: " + str(e) + "\n" + traceback.format_exc())
        return 1