import platform
import pkg_resources
from ctypes.util import find_library
import os

def find_qt_lib(lib_name):
    def get_package_path(package_name, verbose=False):
        try:
            dist = pkg_resources.get_distribution(package_name)
            return dist.location
        except pkg_resources.DistributionNotFound:
            return None

    _lib = find_library(lib_name)
    if _lib:
        if verbose:
            print("Found {} library in system at {}".format(lib_name, _lib))
        return

    print("Did not find {} library in the standard search path. Attempting to find it in PySide.".format(lib_name))
    pyside_path = get_package_path('PySide6')
    if not pyside_path:
        print("Could not find PySide6 package. Aborting.")
        exit(1)

    _plat = platform.system()
    if _plat == "Windows":
        qt6_path = pyside_path
        print ("Adding to PATH: {}".format(qt6_path))
        os.environ['PATH'] = qt6_path + ':' + os.environ.get('PATH', '')
    elif _plat == "Linux":
        qt6_path = pyside_path + '/Qt/lib'
        print ("Adding to LD_LIBRARY_PATH: {}".format(qt6_path))
        os.environ['LD_LIBRARY_PATH'] = qt6_path + ':' + os.environ.get('LD_LIBRARY_PATH', '')
    elif _plat == "Darwin":
        qt6_path = pyside_path + '/Qt/lib'
        print ("Adding to DYLD_LIBRARY_PATH: {}".format(qt6_path))
        os.environ['DYLD_LIBRARY_PATH'] = qt6_path + ':' + os.environ.get('DYLD_LIBRARY_PATH', '')
    
    # Now we should be able to find the library
    _lib = find_library(lib_name)
    if not _lib:
        raise Exception("Could not find {} library in system or PySide. Aborting.".format(lib_name))
    if verbose:
        print("Found {} library in PySide at {}".format(lib_name, _lib))