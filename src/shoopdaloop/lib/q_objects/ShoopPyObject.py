from PySide6.QtQuick import QQuickItem, QQuickPaintedItem
from PySide6.QtCore import QObject, Property, Signal, Slot, QThread, SignalInstance
from PySide6.QtGui import QGuiApplication
from PySide6.QtWidgets import QApplication

from shiboken6 import Shiboken

from ..logging import Logger

from threading import Lock
import functools
import traceback
from typing import *

ids_lock = Lock()
next_obj_id = 1

class ShoopPyObjectImpl:
    def __init__(self, parent_class_name, child_class_name, *args, **kwargs):
        global next_obj_id
        global ids_lock
        ids_lock.acquire()
        self._obj_id = next_obj_id
        next_obj_id += 1
        ids_lock.release()

        self.parent_class_name = parent_class_name
        self.child_class_name = child_class_name

        self._shoop_py_obj_logger = Logger('Frontend.ShoopPyObject')
        self._shoop_py_obj_logger.trace(lambda: "New {} instance ({})".format(self.child_class_name, self.parent_class_name), _id=self._obj_id)

    def isValid(self):
        return Shiboken.isValid(self)
    
    def __del__(self):
        self._shoop_py_obj_logger.trace(lambda: "{} instance: Python __del__ (refcount 0).".format(self.child_class_name, self._obj_id), _id=self._obj_id)

def create_shoop_py_object_class(parent_class):
    class SubClass(ShoopPyObjectImpl, parent_class):
        def __init__(self, *args, **kwargs):
            ShoopPyObjectImpl.__init__(self, parent_class.__name__, self.__class__.__name__, *args, **kwargs)
            parent_class.__init__(self, *args, **kwargs)
            self.destroyed.connect(lambda: self._shoop_py_obj_logger.trace(lambda: "{} instance: QObject destroyed()".format(self.child_class_name), _id=self._obj_id))
    return SubClass

# Thread protection
#
# It seems the Qt JS engine doesn't always like when properties/signals/slots are ran from
# threads other than the GUI thread. In general, we want to tightly control and sanity-check
# the current execution thread.
#
# Therefore we have some wrappers here which can be used to ensure, unless explicitly disabled,
# that slots/properties/signals only run on the QThread the object lives on.

# Wrapper for thread protection (see general note above)

def check_thread_and_report(self, entity_name):
    current_thread = QThread.currentThread()
    try:
        object_thread = self.thread()
        if object_thread != current_thread:
            newline = '\n'
            self._shoop_py_obj_logger.error(
f"""{entity_name} is running in another thread than the object lives on ({current_thread} vs. {object_thread}).
Traceback:
{newline.join(traceback.format_stack())}
""")
    except Exception as e:
        self._shoop_py_obj_logger.warning(f"{entity_name}: Could not determine running thread ({e})")

class ShoopProperty:
    def __init__(self, *args, **kwargs):
        self.__shoop_thread_protect = kwargs.pop('thread_protected', True)
        self.__shoop_prop = Property(*args, **kwargs)
    
    def getter(self, function):
        __self = self
        @functools.wraps(function)
        def wrapper(self, *args, **kwargs):
            if __self.__shoop_thread_protect:
                check_thread_and_report(self, f'Property getter for {function}')
            return function(self, *args, **kwargs)
        return self.__shoop_prop.getter(wrapper)

    def setter(self, function):
        __self = self
        @functools.wraps(function)
        def wrapper(self, *args, **kwargs):
            if __self.__shoop_thread_protect:
                check_thread_and_report(self, f'Property setter for {function}')
            return function(self, *args, **kwargs)
        return self.__shoop_prop.setter(wrapper)
    
    def deleter(self, function):
        __self = self
        @functools.wraps(function)
        def wrapper(self, *args, **kwargs):
            if __self.__shoop_thread_protect:
                check_thread_and_report(self, f'Property deleter for {function}')
            return function(self, *args, **kwargs)
        return self.__shoop_prop.deleter(wrapper)

    def read(self, function):
        __self = self
        @functools.wraps(function)
        def wrapper(self, *args, **kwargs):
            if __self.__shoop_thread_protect:
                check_thread_and_report(self, f'Property reader for {function}')
            return function(self, *args, **kwargs)
        return self.__shoop_prop.read(wrapper)
    
    def write(self, function):
        __self = self
        @functools.wraps(function)
        def wrapper(self, *args, **kwargs):
            if __self.__shoop_thread_protect:
                check_thread_and_report(self, f'Property writer for {function}')
            return function(self, *args, **kwargs)
        return self.__shoop_prop.write(wrapper)

    def __call__(self, function, *args, **kwargs):
        __self = self
        @functools.wraps(function)
        def wrapper(self, *args, **kwargs):
            if __self.__shoop_thread_protect:
                check_thread_and_report(self, f'Property getter for {function}')
            return function(self, *args, **kwargs)
        return self.__shoop_prop.__call__(wrapper, *args, **kwargs)
    
class ShoopSlot:
    def __init__(self, *args, **kwargs):
        self.__shoop_thread_protect = kwargs.pop('thread_protected', True)
        self.__shoop_obj = Slot(*args, **kwargs)

    def __call__(self, function, *args, **kwargs):
        __self = self
        @functools.wraps(function)
        def wrapper(self, *args, **kwargs):
            if __self.__shoop_thread_protect:
                check_thread_and_report(self, f'Slot {function}')
            return function(self, *args, **kwargs)

        return self.__shoop_obj.__call__(wrapper, *args, **kwargs)

def ShoopSignal(*args, **kwargs):
    return Signal(*args, **kwargs)

ShoopQQuickItem = create_shoop_py_object_class(QQuickItem)
ShoopQObject = create_shoop_py_object_class(QObject)
ShoopQGuiApplication = create_shoop_py_object_class(QGuiApplication)
ShoopQQuickPaintedItem = create_shoop_py_object_class(QQuickPaintedItem)
ShoopQApplication = create_shoop_py_object_class(QApplication)
ShoopQThread = create_shoop_py_object_class(QThread)