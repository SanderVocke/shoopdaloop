from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from .ShoopPyObject import *
from .SchemaValidatorImpl import SchemaValidatorImpl

# Wraps a back-end port.
class SchemaValidator(ShoopQObject):
    def __init__(self, parent=None):
        super(SchemaValidator, self).__init__(parent)
        self.impl = SchemaValidatorImpl()

    ###########
    ## SLOTS
    ###########

    # Validate a schema asynchronously.
    @ShoopSlot('QVariantMap', str, str, bool, result=bool)
    def validate_schema(self, obj, obj_desc, schemaname, asynchronous):
        return self.impl.validate_schema(dict(obj), obj_desc, schemaname, asynchronous)