from PySide6.QtQml import QJSValue
from collections.abc import Mapping, Sequence

def recursively_convert_jsvalue(value, max_depth=10):
    converted = dict()

    def convert_single(obj):
        if obj in converted.keys():
            return converted[obj]
        else:
            return obj.toVariant()

    def recursively_convert(obj, depth=0):
        if depth > max_depth:
            raise Exception('maximum recursion depth reached during QJSValue conversion.')
        if isinstance(obj, QJSValue):
            c = convert_single(obj)
            obj = recursively_convert(c, depth+1)
        if isinstance(obj, Mapping):
            for v in obj.values():
                v = recursively_convert(v, depth+1)
        if isinstance(obj, Sequence):
            for v in obj:
                v = recursively_convert(v, depth+1)
        return obj
    
    value = recursively_convert(value)
    return value