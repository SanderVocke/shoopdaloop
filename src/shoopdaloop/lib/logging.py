from shoopdaloop.libshoopdaloop_bindings import destroy_logger, get_logger, shoopdaloop_log, shoopdaloop_should_log
from shoopdaloop.libshoopdaloop_bindings import trace as _trace, debug as _debug, info as _info, warning as _warning, error as _error

from PySide6.QtQml import QJSValue
class Logger:
    def __init__(self, module_name):
        self._name = module_name
        self._backend_handle = get_logger(self._name)
    
    def name(self):
        return self._name
    
    def log(self, msg, level):
        _msg = msg
        if isinstance(_msg, QJSValue):
            if _msg.isCallable():
                _msg = _msg.call().toString()
            else:
                _msg = _msg.toString()
        
        if isinstance(_msg, str):
            shoopdaloop_log(self._backend_handle, level, _msg)
        elif callable(_msg):
            if shoopdaloop_should_log(self._backend_handle, level):
                __msg = _msg()
                if isinstance(__msg, str):
                    shoopdaloop_log(self._backend_handle, level, __msg)
                else:
                    raise ValueError('msg to log must be a string or a callable -> string (callable yielded {})'.format(__msg))
        else:
            raise ValueError('msg to log must be a string or a callable -> string ({})'.format(_msg))
    
    def trace(self, msg):
        self.log(msg, _trace)
    
    def debug(self, msg):
        self.log(msg, _debug)
    
    def info(self, msg):
        self.log(msg, _info)
    
    def warning(self, msg):
        self.log(msg, _warning)
    
    def error(self, msg):
        self.log(msg, _error)
    
    def throw_error(self, msg):
        self.log(msg, _error)
        raise Exception(msg)
    
    def __del__(self):
        if self._backend_handle:
            destroy_logger(self._backend_handle)
        self._backend_handle = None
        self._name = None
