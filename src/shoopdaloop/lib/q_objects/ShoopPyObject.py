from PySide6.QtQuick import QQuickItem, QQuickPaintedItem
from PySide6.QtCore import QObject
from PySide6.QtGui import QGuiApplication

from shiboken6 import Shiboken

from ..logging import Logger

from threading import Lock
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

ShoopQQuickItem = create_shoop_py_object_class(QQuickItem)
ShoopQObject = create_shoop_py_object_class(QObject)
ShoopQGuiApplication = create_shoop_py_object_class(QGuiApplication)
ShoopQQuickPaintedItem = create_shoop_py_object_class(QQuickPaintedItem)