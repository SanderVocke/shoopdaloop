from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
from PyQt6.QtQml import QJSValue
from typing import *

import sys
sys.path.append('../..')

import lib.session_schemas.session_schemas as schemas

# Wraps a back-end port.
class SchemaValidator(QObject):
    def __init__(self, parent=None):
        super(SchemaValidator, self).__init__(parent)
    
    ###########
    ## SLOTS
    ###########

    # Validate a schema. Throws if wrong.
    @pyqtSlot('QVariant', str)
    def validate_schema(self, obj, schemaname):
        _obj = obj
        if isinstance(obj, QJSValue):
            _obj = obj.toVariant()
        schemas.validate_session_object(_obj, schemaname)