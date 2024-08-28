from shoopdaloop.lib.init_dynlibs import init_dynlibs
init_dynlibs()

from shoopdaloop.libshoopdaloop_bindings import destroy_logger, get_logger, shoopdaloop_log, shoopdaloop_should_log
from shoopdaloop.libshoopdaloop_bindings import log_level_debug_trace, log_level_always_trace, log_level_debug, log_level_info, log_level_warning, log_level_error

from PySide6.QtQml import QJSValue

class Logger:
    def __init__(self, module_name):
        self._name = module_name
        self._backend_handle = get_logger(self._name)

    def should_trace(self):
        return shoopdaloop_should_log(self._backend_handle, log_level_always_trace)
    
    def should_debug(self):
        return shoopdaloop_should_log(self._backend_handle, log_level_debug)
    
    def resolve_log_msg(self, value, level):
        if not shoopdaloop_should_log(self._backend_handle, level):
            return None
        
        _msg = value
        if isinstance(_msg, QJSValue):
            if _msg.isCallable():
                _msg = _msg.call().toString()
            else:
                _msg = _msg.toString()
        
        if isinstance(_msg, str):
            return _msg
        elif callable(_msg):
            __msg = _msg()
            if isinstance(__msg, str):
                return __msg
            else:
                raise ValueError('msg to log must be a string or a callable -> string (callable yielded {})'.format(__msg))
        else:
            raise ValueError('msg to log must be a string or a callable -> string ({})'.format(_msg))
    
    def name(self):
        return self._name
    
    def log(self, msg, level, _id=None):
        resolved = self.resolve_log_msg(msg, level)
        if resolved:
            if _id:
                shoopdaloop_log(self._backend_handle, level, '[@{}] {}'.format(_id, resolved))
            else:
                shoopdaloop_log(self._backend_handle, level, resolved)
    
    def trace(self, msg, _id=None):
        self.log(msg, log_level_always_trace, _id)
    
    def debug(self, msg, _id=None):
        self.log(msg, log_level_debug, _id)
    
    def info(self, msg, _id=None):
        self.log(msg, log_level_info, _id)
    
    def warning(self, msg, _id=None):
        self.log(msg, log_level_warning, _id)
    
    def error(self, msg, _id=None):
        self.log(msg, log_level_error, _id)
    
    def throw_error(self, msg, _id=None):
        self.log(msg, log_level_error, _id)
        raise Exception(msg)
    
    def __del__(self):
        # TODO
        #if self._backend_handle:
        #    destroy_logger(self._backend_handle)
        self._backend_handle = None
        self._name = None
