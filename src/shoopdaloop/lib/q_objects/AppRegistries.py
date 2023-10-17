from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer, QEvent, Qt
from PySide6.QtQml import QJSValue

class AppRegistries(QObject):
    def __init__(self, parent=None):
        super(AppRegistries, self).__init__(parent)
        self._objects_registry = None
        self._state_registry = None
        self._fx_chain_states_registry = None
    
    # objects registry
    objectsRegistryChanged = Signal('QVariant')
    @Property('QVariant', notify=objectsRegistryChanged)
    def objects_registry(self):
        return self._objects_registry
    @objects_registry.setter
    def objects_registry(self, l):
        if l != self._objects_registry:
            self._objects_registry = l
            self.objectsRegistryChanged.emit(l)
    
    # state registry
    stateRegistryChanged = Signal('QVariant')
    @Property('QVariant', notify=stateRegistryChanged)
    def state_registry(self):
        return self._state_registry
    @state_registry.setter
    def state_registry(self, l):
        if l != self._state_registry:
            self._state_registry = l
            self.stateRegistryChanged.emit(l)
    
    # FX chain states registry
    fxChainStatesRegistryChanged = Signal('QVariant')
    @Property('QVariant', notify=fxChainStatesRegistryChanged)
    def fx_chain_states_registry(self):
        return self._fx_chain_states_registry
    @fx_chain_states_registry.setter
    def fx_chain_states_registry(self, l):
        if l != self._fx_chain_states_registry:
            self._fx_chain_states_registry = l
            self.fxChainStatesRegistryChanged.emit(l)
    