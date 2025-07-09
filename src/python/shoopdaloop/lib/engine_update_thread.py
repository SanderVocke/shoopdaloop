from shoop_rust import shoop_rust_get_engine_update_thread_addr, shoop_rust_get_engine_update_thread_wrapper_addr
from PySide6.QtCore import QThread, QObject
from shiboken6 import Shiboken

def get_engine_update_thread():
    addr = shoop_rust_get_engine_update_thread_addr()
    inst = Shiboken.wrapInstance(shoop_rust_get_engine_update_thread_addr(), QThread)
    if not isinstance(inst, QThread):
        raise Exception("Created object is not a QThread")
    return inst

def get_engine_update_thread_wrapper():
    addr = shoop_rust_get_engine_update_thread_wrapper_addr()
    inst = Shiboken.wrapInstance(shoop_rust_get_engine_update_thread_wrapper_addr(), QObject)
    if not isinstance(inst, QObject):
        raise Exception("Created object is not a QObject")
    return inst