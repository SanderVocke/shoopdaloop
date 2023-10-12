from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQuick import QQuickItem

from ..logging import Logger as BaseLogger
import jacklib as jack
from jacklib.helpers import *

class JackControlClient(QQuickItem):
    _instance = None
    
    @staticmethod
    def get_instance():
        if not JackControlClient._instance:
            JackControlClient._instance = JackControlClient()
        return JackControlClient._instance
    
    def __init__(self):
        super(JackControlClient, self).__init__(None)
        self.logger = BaseLogger("Frontend.JackControlClient")
        
        status = jack.jack_status_t()
        client = jack.client_open("shoopdaloop-control", jack.JackNoStartServer, status)
        if status.value:
            err = get_jack_status_error_string(status)
            self.logger.error("Could not open jack client: {}".format(err))
        
        self._client = client
        
        jack.set_port_registration_callback(self._client, self.port_register_cb, None)
        jack.set_port_rename_callback(self._client, self.port_rename_cb, None)
    
    def __del__(self):
        if self._client:
            jack.client_close(self._client)
    
    def port_register_cb(self, port_id, registered_or_unregistered, *args):
        port = jack.port_by_id(self._client, port_id)
        name = jack.port_name(port)
        if registered_or_unregistered:
            self.portRegistered.emit(name)
        else:
            self.portUnregistered.emit(name)
    
    def port_rename_cb(self, port_id, old_name, new_name, *args):
        self.portRenamed.emit(old_name, new_name)
    
    ######################
    ## SIGNALS
    ######################
    portRegistered = Signal(str)
    portUnregistered = Signal(str)
    portRenamed = Signal(str, str)

    ######################
    # PROPERTIES
    ######################
    
    ###########
    ## SLOTS
    ###########

    @Slot(str, str)
    def connect_ports(self, port_from, port_to):
        self.logger.debug("Connecting {} to {}".format(port_from, port_to))
        jack.connect(self._client, port_from, port_to)
    
    @Slot(str, str)
    def disconnect_ports(self, port_from, port_to):
        self.logger.debug("Disconnecting {} from {}".format(port_from, port_to))
        jack.disconnect(self._client, port_from, port_to)
        
    @Slot('QVariant', 'QVariant', int, result=list)
    def find_ports(self, maybe_name_regex=None, maybe_type_regex=None, flags=0):
        ports = []
        ports_ptr = jack.get_ports(self._client, maybe_name_regex, maybe_type_regex, flags)
        i = 0
        while ports_ptr[i]:
            ports.append(ports_ptr[i].decode('ascii'))
            i += 1
        return ports

    ###########
    ## METHODS
    ###########