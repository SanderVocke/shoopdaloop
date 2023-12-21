from typing import *
import sys
import os

from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQml import QJSValue

from .ShoopPyObject import *

from ..logging import *

# Wraps a back-end port.
class OSUtils(ShoopQObject):
    def __init__(self, parent=None):
        super(OSUtils, self).__init__(parent)
    
    ###########
    ## SLOTS
    ###########

    @Slot(str, result='QVariant')
    def get_env(self, varname):
        return os.environ.get(varname, None)