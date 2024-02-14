from typing import *
import sys

from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQml import QJSValue

from .ShoopPyObject import *

from ..session_schemas.session_schemas import validate_session_object
import json
from ..logging import *

# Wraps a back-end port.
class SchemaValidator(ShoopQObject):
    def __init__(self, parent=None):
        super(SchemaValidator, self).__init__(parent)
        self.logger = Logger("Frontend.SchemaValidator")
    
    ###########
    ## SLOTS
    ###########

    # Validate a schema. Throws if wrong.
    @ShoopSlot('QVariant', str)
    def validate_schema(self, obj, schemaname):
        _obj = obj
        if isinstance(obj, QJSValue):
            _obj = obj.toVariant()
        self.logger.trace(lambda: "Validating against {}: {}".format(schemaname, json.dumps(_obj, indent=2)))
        validate_session_object(_obj, schemaname)