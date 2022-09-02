from typing import *
import sys

from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQml import QJSValue

from ..session_schemas.session_schemas import validate_session_object

# Wraps a back-end port.
class SchemaValidator(QObject):
    def __init__(self, parent=None):
        super(SchemaValidator, self).__init__(parent)
    
    ###########
    ## SLOTS
    ###########

    # Validate a schema. Throws if wrong.
    @Slot('QVariant', str)
    def validate_schema(self, obj, schemaname):
        _obj = obj
        if isinstance(obj, QJSValue):
            _obj = obj.toVariant()
        validate_session_object(_obj, schemaname)