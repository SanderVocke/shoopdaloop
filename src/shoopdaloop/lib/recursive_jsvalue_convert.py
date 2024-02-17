from PySide6.QtQml import QJSValue
from collections.abc import Mapping, Sequence

def recursively_convert_jsvalue(value):
    converted = dict()

    def convert_single(obj):
        if obj in converted.keys():
            return converted[obj]
        else:
            return obj.toVariant()

    def recursively_convert(obj):
        if isinstance(obj, QJSValue):
            c = convert_single(obj)
            obj = recursively_convert(c)
        if isinstance(obj, Mapping):
            for v in obj.values():
                v = recursively_convert(v)
        if isinstance(obj, Sequence):
            for v in obj:
                v = recursively_convert(v)
        return obj
    
    value = recursively_convert(value)
    return value