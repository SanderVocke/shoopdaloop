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
    pyside_path = getsitepackages() + '/PySide6'

    _plat = platform.system()
    if _plat == "Windows":
        qt6_path = pyside_path
        logger.debug ("Adding to PATH: {}".format(qt6_path))
        os.environ['PATH'] = qt6_path + ':' + os.environ.get('PATH', '')
    elif _plat == "Linux":
        qt6_path = pyside_path + '/Qt/lib'
        logger.debug ("Adding to LD_LIBRARY_PATH: {}".format(qt6_path))
        os.environ['LD_LIBRARY_PATH'] = qt6_path + ':' + os.environ.get('LD_LIBRARY_PATH', '')
    elif _plat == "Darwin":
        qt6_path = pyside_path + '/Qt/lib'
        logger.debug ("Adding to DYLD_LIBRARY_PATH: {}".format(qt6_path))
        os.environ['DYLD_LIBRARY_PATH'] = qt6_path + ':' + os.environ.get('DYLD_LIBRARY_PATH', '')
    
    # Now we should be able to find the library
    _lib = find_library(lib_name)
    if not _lib:
        raise Exception("Could not find {} library in system or PySide. Aborting.".format(lib_name))
    logger.debug("Found {} library in PySide at {}".format(lib_name, _lib))