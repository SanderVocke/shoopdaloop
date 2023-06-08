from backend_interface import *

class Logger:
    def __init__(self, module_name):
        self._name = module_name
        self._backend_handle = get_logger(self._name)
    
    def name(self):
        return self._name
    
    def trace(self, msg):
        shoopdaloop_log(self._backend_handle, backend_interface.trace, msg)
    
    def debug(self, msg):
        shoopdaloop_log(self._backend_handle, backend_interface.debug, msg)
    
    def info(self, msg):
        shoopdaloop_log(self._backend_handle, backend_interface.info, msg)
    
    def warning(self, msg):
        shoopdaloop_log(self._backend_handle, backend_interface.warning, msg)
    
    def error(self, msg):
        shoopdaloop_log(self._backend_handle, backend_interface.error, msg)
