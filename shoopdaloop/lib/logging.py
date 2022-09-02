from .backend_interface import get_logger, shoopdaloop_log
from .backend_interface import trace as _trace, debug as _debug, info as _info, warning as _warning, error as _error

class Logger:
    def __init__(self, module_name):
        self._name = module_name
        self._backend_handle = get_logger(self._name)
    
    def name(self):
        return self._name
    
    def trace(self, msg):
        shoopdaloop_log(self._backend_handle, _trace, msg)
    
    def debug(self, msg):
        shoopdaloop_log(self._backend_handle, _debug, msg)
    
    def info(self, msg):
        shoopdaloop_log(self._backend_handle, _info, msg)
    
    def warning(self, msg):
        shoopdaloop_log(self._backend_handle, _warning, msg)
    
    def error(self, msg):
        shoopdaloop_log(self._backend_handle, _error, msg)
    
    def throw_error(self, msg):
        shoopdaloop_log(self._backend_handle, _error, msg)
        raise Exception(msg)
