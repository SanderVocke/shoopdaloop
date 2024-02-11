from typing import *
import sys

from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer

from .ShoopPyObject import *

from ..session_schemas.session_schemas import validate_session_object
from ..resolve_qjsvalue import resolve_jsvalue_if_any
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
    @Slot('QVariant', str)
    def validate_schema(self, obj, schemaname):
        _obj = resolve_jsvalue_if_any(obj)
        self.logger.trace(lambda: "Validating against {}: {}".format(schemaname, json.dumps(_obj, indent=2)))
        validate_session_object(_obj, schemaname)