import os
import sys
from ctypes import *
from shoopdaloop.lib.logging import Logger
import platform
import site

def ensure_ld_qt():
    already_spawned = (os.environ.get('_SHOOP_ENSURE_LD_QT', '') == 'child')

    _system = platform.system()
    logger = Logger('Frontend.EnsureLoadQt')
    vars = {
        'Linux': 'LD_LIBRARY_PATH',
        'Windows': 'PATH',
        'Darwin': 'DYLD_LIBRARY_PATH'
    }
    separators = {
        'Linux': ':',
        'Windows': ';',
        'Darwin': ':'
    }
    prefixes = {
        'Linux': 'lib',
        'Windows': '',
        'Darwin': 'lib'
    }
    postfixes = {
        'Linux': '.so.6',
        'Windows': '.dll',
        'Darwin': '.dylib'
    }

    required_lib_bases = [ 'Qt6Core', 'Qt6Quick', 'Qt6QuickTest', 'Qt6Qml' ]
    required_libs = [
        prefixes[_system] + p + postfixes[_system] for p in required_lib_bases
    ]

    extrapaths = []
    pyside6_paths = [ p + '/PySide6' for p in site.getsitepackages() ]
    if _system == 'Windows':
        extrapaths = pyside6_paths
    elif _system == 'Linux':
        extrapaths = [ p + '/Qt/lib' for p in pyside6_paths ]
    elif _system == 'Darwin':
        # TODO
        extrapaths = [ p + '/Qt/lib' for p in pyside6_paths ]

    sep = separators[_system]
    var = vars[_system]
    new_var_value = os.environ.get(var, '') + sep + sep.join(extrapaths)

    def all_found():
        for lib in required_libs:
            try:
                logger.debug('Trying to load {}...'.format(lib))
                CDLL(lib)
                logger.debug('...Success.')
            except Exception as e:
                logger.debug('...Could not load {}: {}'.format(lib, e))
                return False
        return True

    if all_found():
        logger.debug('All Qt libraries available.')
    else:
        if not already_spawned:
            logger.debug('Some Qt libraries are missing. Setting {} to {}.'.format(vars[_system], new_var_value))
            os.environ[vars[_system]] = new_var_value
            os.environ['_SHOOP_ENSURE_LD_QT'] = 'child'
            logger.debug('Restarting process to ensure Qt libraries are loaded.')
            os.execv(sys.executable, [sys.executable] + sys.argv)
            exit(0)
        else:
            logger.error('Failed to find Qt libraries after restart. Aborting.')
            exit(1)