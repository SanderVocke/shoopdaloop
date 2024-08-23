import shiboken6
import ctypes
import platform

from PySide6.QtCore import QObject

from .directories import *

g_shoop_rust_create_autoconnect = None
g_lib = None

def load_lib():
    global g_lib
    global g_shoop_rust_create_autoconnect
    if g_lib:
        return
    import ctypes
    dylib_name = (
        'shoop_rs_frontend.dll' if platform.system() == "Windows"
        else ('libshoop_rs_frontend.dylib' if platform.system() == "Darwin"
        else 'libshoop_rs_frontend.so')
    )
    g_lib = ctypes.CDLL(installation_dir() + '/' + dylib_name)

    if not hasattr(g_lib, "shoop_rust_init"):
        raise Exception("shoop_rust_init not found")

    g_lib.shoop_rust_create_autoconnect.argtypes = []
    g_lib.shoop_rust_create_autoconnect.restype = ctypes.c_void_p
    g_shoop_rust_create_autoconnect = lambda: shiboken6.wrapInstance(g_lib.shoop_rust_create_autoconnect(), QObject)

def shoop_rust_init():
    global g_lib
    load_lib()

    g_lib.shoop_rust_init.argtypes = []
    g_lib.shoop_rust_init.restype = None
    g_lib.shoop_rust_init()

def shoop_rust_create_autoconnect():
    global g_shoop_rust_create_autoconnect
    if not g_shoop_rust_create_autoconnect:
        load_lib()
    return g_shoop_rust_create_autoconnect()