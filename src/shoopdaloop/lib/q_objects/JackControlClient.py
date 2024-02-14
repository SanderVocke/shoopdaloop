from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer, Qt
from PySide6.QtQuick import QQuickItem

from .ShoopPyObject import *

from ..logging import Logger as BaseLogger
import jacklib as jack
from jacklib.helpers import *

class JackControlClient(ShoopQQuickItem):
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
            self.logger.error(lambda: "Could not open jack client: {}".format(err))
        
        self._client = client
        
        self.portRenamedProxy.connect(self.portRenamed, Qt.ConnectionType.QueuedConnection)
        self.portRegisteredProxy.connect(self.portRegistered, Qt.ConnectionType.QueuedConnection)
        self.portUnregisteredProxy.connect(self.portUnregistered, Qt.ConnectionType.QueuedConnection)
        
        jack.set_port_registration_callback(self._client, self.port_register_cb, None)
        jack.set_port_rename_callback(self._client, self.port_rename_cb, None)
    
    def __del__(self):
        if self._client:
            jack.client_close(self._client)
    
    def port_register_cb(self, port_id, registered_or_unregistered, *args):
        port = jack.port_by_id(self._client, port_id)
        name = jack.port_name(port)
        if registered_or_unregistered:
            self.portRegisteredProxy.emit(name)
        else:
            self.portUnregisteredProxy.emit(name)
    
    def port_rename_cb(self, port_id, old_name, new_name, *args):
        self.portRenamedProxy.emit(old_name, new_name)
    
    ######################
    ## SIGNALS
    ######################
    portRegistered = ShoopSignal(str)
    portUnregistered = ShoopSignal(str)
    portRenamed = ShoopSignal(str, str)

    # These proxies ensure decoupling through the Qt event loop.
    portRegisteredProxy = ShoopSignal(str)
    portUnregisteredProxy = ShoopSignal(str)
    portRenamedProxy = ShoopSignal(str, str)

    ######################
    # PROPERTIES
    ######################
    
    ###########
    ## SLOTS
    ###########

    @ShoopSlot(str, str)
    def connect_ports(self, port_from, port_to):
        self.logger.debug(lambda: "Connecting {} to {}".format(port_from, port_to))
        jack.connect(self._client, port_from, port_to)
    
    @ShoopSlot(str, str)
    def disconnect_ports(self, port_from, port_to):
        self.logger.debug(lambda: "Disconnecting {} from {}".format(port_from, port_to))
        jack.disconnect(self._client, port_from, port_to)
        
    @ShoopSlot('QVariant', 'QVariant', int, result=list)
    def find_ports(self, maybe_name_regex=None, maybe_type_regex=None, flags=0):
        ports = c_char_p_p_to_list(jack.get_ports(self._client, maybe_name_regex, maybe_type_regex, flags))
        return ports
    
    @ShoopSlot(str, result=bool)
    def port_is_mine(self, port_name):
        port = jack.port_by_name(self._client, port_name)
        if port:
            return bool(jack.port_is_mine(self._client, port))
        return False

    @ShoopSlot(str, result=list)
    def all_port_connections(self, port_name):
        port = jack.port_by_name(self._client, port_name)
        if port:
            conns = list(jack.port_get_all_connections(self._client, port))
            return conns
        return []

    ###########
    ## METHODS
    ###########