from PySide6.QtQml import QJSValue

from .logging import Logger

logger = Logger('Frontend.ResolveQJSValue')

def resolve_jsvalue_if_any(val, failure_val=None, max_recursion=5):
    global logger
    if not isinstance(val, QJSValue):
        logger.debug(lambda: f'no conversion needed for {val}')
        return val
    
    def convert_fn(v, iter=0):
        r = v.toVariant()
        if isinstance(r, QJSValue):
            if iter >= max_recursion:
                logger.error(lambda: f'Could not convert QJSValue in {max_recursion} recursions')
                return failure_val
            return convert_fn(v, iter+1)
        return r
    
    rval = convert_fn(val)
    logger.debug(lambda: f'Converted {val} to {rval}')
    return rval