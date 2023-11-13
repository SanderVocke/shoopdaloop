import platform
from site import getsitepackages
from ctypes.util import find_library
import os

from shoopdaloop.lib.logging import Logger

logger = Logger("Frontend.FindQtLib")

def find_qt_lib(lib_name):
    def get_package_path(package_name):
        try:
            dist = pkg_resources.get_distribution(package_name)
            return dist.location
        except pkg_resources.DistributionNotFound:
            return None

    _lib = find_library(lib_name)
    if _lib:
        logger.debug("Found {} library in system at {}".format(lib_name, _lib))
        return

    logger.debug("Did not find {} library in the standard search path. Attempting to find it in PySide.".format(lib_name))
    pyside_paths = [p + '/PySide6' for p in getsitepackages()]

    _plat = platform.system()
    if _plat == "Windows":
        os.environ['PATH'] = ':'.join(pyside_paths) + ';' + os.environ.get('PATH', '')
        logger.debug("Modified PATH: {}".format(os.environ.get('PATH')))
    elif _plat == "Linux":
        os.environ['LD_LIBRARY_PATH'] = ':'.join([p + '/Qt/lib' for p in pyside_paths]) + ':' + os.environ.get('LD_LIBRARY_PATH', '')
        logger.debug("Modified LD_LIBRARY_PATH: {}".format(os.environ.get('LD_LIBRARY_PATH')))
    elif _plat == "Darwin":
        os.environ['DYLD_LIBRARY_PATH'] = ':'.join([p + '/Qt/lib' for p in pyside_paths]) + ':' + os.environ.get('DYLD_LIBRARY_PATH', '')
        logger.debug("Modified DYLD_LIBRARY_PATH: {}".format(os.environ.get('DYLD_LIBRARY_PATH')))
    
    # Now we should be able to find the library
    _lib = find_library(lib_name)
    if not _lib:
        raise Exception("Could not find {} library in system or PySide. Aborting.".format(lib_name))
    logger.debug("Found {} library in PySide at {}".format(lib_name, _lib))