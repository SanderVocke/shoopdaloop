from PySide6.QtCore import Slot

import os

from .ShoopPyObject import ShoopQObject

class EnvHelper(ShoopQObject):
    def __init__(self, parent=None):
        super(EnvHelper, self).__init__(parent)
    
    @Slot(str, 'QVariant')
    def set_env(self, key, value):
        os.environ[key] = value
    
    @Slot(str, result='QVariant')
    def get_env(self, key):
        return os.environ[key]