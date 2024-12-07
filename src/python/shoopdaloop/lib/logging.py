from shoopdaloop.lib.init_dynlibs import init_dynlibs
init_dynlibs()

from PySide6.QtQml import QJSValue

import shoop_py_backend

class Logger:
    def __init__(self, module_name):
        self._name = module_name
        self._backend_handle = shoop_py_backend.Logger(self._name)

    def should_trace(self):
        return shoop_py_backend.should_log(self._backend_handle, shoop_py_backend.LogLevel.AlwaysTrace)
    
    def should_debug(self):
        return shoop_py_backend.should_log(self._backend_handle, shoop_py_backend.LogLevel.Debug)
    
    def resolve_log_msg(self, value, level):
        if not shoop_py_backend.should_log(self._backend_handle, shoop_py_backend.LogLevel(level)):
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
                shoop_py_backend.log(self._backend_handle, shoop_py_backend.LogLevel(level), '[@{}] {}'.format(_id, resolved))
            else:
                shoop_py_backend.log(self._backend_handle, shoop_py_backend.LogLevel(level), resolved)
    
    def trace(self, msg, _id=None):
        self.log(msg, shoop_py_backend.LogLevel.AlwaysTrace, _id)
    
    def debug(self, msg, _id=None):
        self.log(msg, shoop_py_backend.LogLevel.Debug, _id)
    
    def info(self, msg, _id=None):
        self.log(msg, shoop_py_backend.LogLevel.Info, _id)
    
    def warning(self, msg, _id=None):
        self.log(msg, shoop_py_backend.LogLevel.Warning, _id)
    
    def error(self, msg, _id=None):
        self.log(msg, shoop_py_backend.LogLevel.Error, _id)
    
    def throw_error(self, msg, _id=None):
        self.log(msg, shoop_py_backend.LogLevel.Error, _id)
        raise Exception(msg)
    
    def __del__(self):
        # TODO
        #if self._backend_handle:
        #    destroy_logger(self._backend_handle)
        self._backend_handle = None
        self._name = None
