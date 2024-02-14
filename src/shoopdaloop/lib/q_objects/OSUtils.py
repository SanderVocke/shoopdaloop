from typing import *
import sys
import os

from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQml import QJSValue

from .ShoopPyObject import *

from ..logging import *
from ..crash_handling import test_exception, test_segfault, test_abort

# Wraps a back-end port.
class OSUtils(ShoopQObject):
    def __init__(self, parent=None):
        super(OSUtils, self).__init__(parent)
    
    ###########
    ## SLOTS
    ###########

    @ShoopSlot(str, result='QVariant')
    def get_env(self, varname):
        return os.environ.get(varname, None)
    
    @ShoopSlot()
    def test_segfault(self):
        test_segfault()
    
    @ShoopSlot()
    def test_exception(self):
        test_exception()
    
    @ShoopSlot()
    def test_abort(self):
        test_abort()