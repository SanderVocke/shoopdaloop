from PySide6.QtQuick import QQuickItem, QQuickPaintedItem
from PySide6.QtCore import QObject, Property, Signal, Slot, QThread, SignalInstance, QMetaObject, Qt
from PySide6.QtGui import QGuiApplication
from PySide6.QtWidgets import QApplication

from shiboken6 import Shiboken

from ..logging import Logger

from threading import Lock
import functools
import traceback
from typing import *
from enum import *

ids_lock = Lock()
next_obj_id = 1

class ThreadProtectionType(Enum):
    # The signal/slot/property may be used from any thread.
    AnyThread = 0

    # The signal/slot/property may only be used on the thread the owner lives on.
    ObjectThread = 1

    # The signal/slot/property may only be used on threads other than the one the owner lives on.
    OtherThread = 2

thread_protected_signals = dict()

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
            global thread_protected_signals

            # Superclass initializations
            ShoopPyObjectImpl.__init__(self, parent_class.__name__, self.__class__.__name__, *args, **kwargs)
            parent_class.__init__(self, *args, **kwargs)

            # Destroyed notification
            self.destroyed.connect(lambda: self._shoop_py_obj_logger.trace(lambda: "{} instance: QObject destroyed()".format(self.child_class_name), _id=self._obj_id))

            class_attrs = vars(self.__class__)
            # Iterate over the class attributes and determine if they are thread-protected Signals.
            # if so, connect to their signal instances a thread-protection function
            for attr_name, attr_value in class_attrs.items():
                protection = thread_protected_signals.get(attr_value, None) if isinstance(attr_value, Signal) else None
                if protection:
                    protection = thread_protected_signals[attr_value]
                    signal_instance = attr_value.__get__(self, self.__class__)
                    fmtstring = f'Signal emit of "{attr_name}" from {self}'
                    def check_thread():
                        check_thread_and_report(self, protection, fmtstring)
                    signal_instance.connect(lambda: check_thread(), Qt.DirectConnection)
    
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

def check_thread_and_report(self, protection, entity_name):
    if not self.isValid():
        return
    current_thread = QThread.currentThread()
    try:
        object_thread = self.thread()
        if protection == ThreadProtectionType.ObjectThread and object_thread != current_thread:
            newline = '\n'
            self._shoop_py_obj_logger.error(
f"""{entity_name} is running in another thread than the object lives on ({current_thread} vs. {object_thread}).
Traceback:
{newline.join(traceback.format_stack())}
""")
        elif protection == ThreadProtectionType.OtherThread and object_thread == current_thread:
            newline = '\n'
            self._shoop_py_obj_logger.error(
f"""{entity_name} is running on its owner object's thread while it is not allowed to ({current_thread} vs. {object_thread}).
Traceback:
{newline.join(traceback.format_stack())}
""")
    except Exception as e:
        self._shoop_py_obj_logger.warning(f"{entity_name}: Could not determine running thread ({e})")

class ShoopProperty:
    def __init__(self, *args, **kwargs):
        self.__shoop_thread_protect = kwargs.pop('thread_protection', ThreadProtectionType.ObjectThread)
        self.__shoop_prop = Property(*args, **kwargs)
    
    def getter(self, function):
        __self = self
        @functools.wraps(function)
        def wrapper(self, *args, **kwargs):
            check_thread_and_report(self, __self.__shoop_thread_protect, f'Property getter for {function}')
            return function(self, *args, **kwargs)
        return self.__shoop_prop.getter(wrapper)

    def setter(self, function):
        __self = self
        @functools.wraps(function)
        def wrapper(self, *args, **kwargs):
            check_thread_and_report(self, __self.__shoop_thread_protect, f'Property setter for {function}')
            return function(self, *args, **kwargs)
        return self.__shoop_prop.setter(wrapper)
    
    def deleter(self, function):
        __self = self
        @functools.wraps(function)
        def wrapper(self, *args, **kwargs):
            check_thread_and_report(self, __self.__shoop_thread_protect, f'Property deleter for {function}')
            return function(self, *args, **kwargs)
        return self.__shoop_prop.deleter(wrapper)

    def read(self, function):
        __self = self
        @functools.wraps(function)
        def wrapper(self, *args, **kwargs):
            check_thread_and_report(self, __self.__shoop_thread_protect, f'Property reader for {function}')
            return function(self, *args, **kwargs)
        return self.__shoop_prop.read(wrapper)
    
    def write(self, function):
        __self = self
        @functools.wraps(function)
        def wrapper(self, *args, **kwargs):
            check_thread_and_report(self, __self.__shoop_thread_protect, f'Property writer for {function}')
            return function(self, *args, **kwargs)
        return self.__shoop_prop.write(wrapper)

    def __call__(self, function, *args, **kwargs):
        __self = self
        @functools.wraps(function)
        def wrapper(self, *args, **kwargs):
            check_thread_and_report(self, __self.__shoop_thread_protect, f'Property getter for {function}')
            return function(self, *args, **kwargs)
        return self.__shoop_prop.__call__(wrapper, *args, **kwargs)

def ShoopSlot(*args, **kwargs):
    thread_protection = kwargs.pop('thread_protection', ThreadProtectionType.ObjectThread)
    slot_decorator = Slot(*args, **kwargs)
    def decorator(fn):
        @functools.wraps(fn)
        def wrapper(self, *args, **kwargs):
            check_thread_and_report(self, thread_protection, f'Slot {fn}')
            return fn(self, *args, **kwargs)
        return slot_decorator(wrapper)
    return decorator

def ShoopSignal(*args, **kwargs):
    global thread_protected_signals
    thread_protection = kwargs.pop('thread_protection', ThreadProtectionType.ObjectThread)
    rval = Signal(*args, **kwargs)
    if thread_protection != ThreadProtectionType.AnyThread:
        thread_protected_signals[rval] = thread_protection
    return rval

ShoopQQuickItem = create_shoop_py_object_class(QQuickItem)
ShoopQObject = create_shoop_py_object_class(QObject)
ShoopQGuiApplication = create_shoop_py_object_class(QGuiApplication)
ShoopQQuickPaintedItem = create_shoop_py_object_class(QQuickPaintedItem)
ShoopQApplication = create_shoop_py_object_class(QApplication)
ShoopQThread = create_shoop_py_object_class(QThread)

# Convenience class. Has a single signal called "signal" which bypasses
# our thread introspection and can emit from whatever thread without
# warnings.
class ThreadUnsafeSignalEmitter(ShoopQObject):
    def __init__(self, parent=None):
        super(ThreadUnsafeSignalEmitter, self).__init__(parent)
    
    signal = ShoopSignal(thread_protection = ThreadProtectionType.AnyThread)
    
    def do_emit(self, *args, **kwargs):
        if self.isValid():
            self.signal.emit()