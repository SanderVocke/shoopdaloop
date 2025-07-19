from ..logging import Logger as BaseLogger

class LoggerImpl():
    def __init__(self, obj_id):
        self.logger = BaseLogger("Frontend.Unknown")
        self._id = None
        self._id_to_print = obj_id
        self._obj_id = obj_id

    # name
    def get_name(self):
        return self.logger.name()
    def set_name(self, value: str):
        if value and value != self.logger.name():
            self.logger = BaseLogger(value)
            return True
        return False
    
    # instance identifier
    def get_instance_identifier(self):
        return self._id
    def set_instance_identifier(self, value: str):
        if value and value != self._id:
            self._id = value
            self._id_to_print = str(self._obj_id) + ":" + value
            return True
        return False
        
    ###########
    ## SLOTS
    ###########

    def trace(self, msg):
        self.logger.trace(msg, _id=self._id_to_print)
    
    def debug(self, msg):
        self.logger.debug(msg, _id=self._id_to_print)
    
    def info(self, msg):
        self.logger.info(msg, _id=self._id_to_print)
    
    def warning(self, msg):
        self.logger.warning(msg, _id=self._id_to_print)
    
    def error(self, msg):
        self.logger.error(msg, _id=self._id_to_print)
    
    def throw_error(self, msg):
        self.logger.error(msg, _id=self._id_to_print)
        raise Exception(msg)
    
    def should_trace(self):
        return self.logger.should_trace()
    
    def should_debug(self):
        return self.logger.should_debug()
