from typing import *
import sys
from shoopdaloop.lib.logging import *
from shoopdaloop.lib.validate_session_schemas import validate_session_object
import json
import threading
import copy

# Wraps a back-end port.
class SchemaValidatorImpl():
    def __init__(self):
        self.logger = Logger("Frontend.SchemaValidator")

    ###########
    ## SLOTS
    ###########

    # Validate a schema asynchronously.
    def validate_schema(self, obj, obj_desc, schemaname, asynchronous):
        _obj = obj

        if type(_obj) not in [list, dict]:
            self.logger.error(lambda: f"Cannot validate a non-list/dict object: {type(_obj)}")

        cpy = copy.deepcopy(_obj)
        obj_desc_copy = copy.deepcopy(obj_desc)
        schemaname_copy = copy.deepcopy(schemaname)
        def validate_fn(obj=cpy, obj_desc=obj_desc_copy, schemaname=schemaname_copy):
            try:
                self.logger.trace(lambda: f"Validating against {schemaname}: {json.dumps(obj, indent=2)}")
                validate_session_object(obj, schemaname)
                return True
            except Exception as e:
                self.logger.error(lambda: f"Error validating {obj_desc} against {schemaname}: {e}")
                return False

        if asynchronous:
            threading.Thread(target=validate_fn).start()
            return True
        return validate_fn()