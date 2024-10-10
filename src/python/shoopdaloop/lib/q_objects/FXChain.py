from typing import *

from PySide6.QtCore import Qt
from PySide6.QtQuick import QQuickItem

from .FindParentBackend import FindParentBackend
from .ShoopPyObject import *

from ..backend_wrappers import *
from ..q_objects.Backend import Backend
from ..findFirstParent import findFirstParent
from ..logging import Logger

# Wraps a back-end FX chain.
class FXChain(FindParentBackend):
    def __init__(self, parent=None):
        super(FXChain, self).__init__(parent)
        self._active = self._new_active = True
        self._ready = self._new_ready = False
        self._ui_visible = self._new_ui_visible = False
        self._initialized = False
        self._backend_object = None
        self._chain_type = self._new_chain_type = None
        self._title = self._new_title = ""
        self._n_pending_updates = 0
        self.logger = Logger('Frontend.FXChain')

        self.backendChanged.connect(lambda: self.maybe_initialize())
        self.backendInitializedChanged.connect(lambda: self.maybe_initialize())
        
        self._signal_sender = ThreadUnsafeSignalEmitter()
        self._signal_sender.signal.connect(self.updateOnGuiThread, Qt.QueuedConnection)

    ######################
    # PROPERTIES
    ######################
    
    # initialized
    initializedChanged = ShoopSignal(bool)
    @ShoopProperty(bool, notify=initializedChanged)
    def initialized(self):
        return self._initialized

    # ui_visible
    uiVisibleChanged = ShoopSignal(bool)
    @ShoopProperty(bool, notify=uiVisibleChanged)
    def ui_visible(self):
        return self._ui_visible
    # Indirect setter via back-end
    @ShoopSlot(bool)
    def set_ui_visible(self, value):
        if self._backend_object:
            self._backend_object.set_visible(value)
        else:
            self.initializedChanged.connect(lambda: self._backend_object.set_visible(value))
    
    # title
    titleChanged = ShoopSignal(str)
    @ShoopProperty(str, notify=titleChanged)
    def title(self):
        return self._title
    @title.setter
    def title(self, l):
        if l and l != self._title:
            self._title = l
            self.titleChanged.emit(l)
    
    # ready
    readyChanged = ShoopSignal(bool)
    @ShoopProperty(bool, notify=readyChanged)
    def ready(self):
        return self._ready
    
    # active
    activeChanged = ShoopSignal(bool)
    @ShoopProperty(bool, notify=activeChanged)
    def active(self):
        return self._active
    # Indirect setter via back-end
    @ShoopSlot(bool)
    def set_active(self, value):
        if self._backend_object:
            self._backend_object.set_active(value)
        else:
            self.initializedChanged.connect(lambda: self._backend_object.set_active(value))
    
    # chain type
    chainTypeChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=chainTypeChanged)
    def chain_type(self):
        return (self._chain_type if self._chain_type != None else -1)
    @chain_type.setter
    def chain_type(self, l):
        if l != None and l != self._chain_type:
            if self._chain_type:
                self.logger.throw_error('May not change chain type of existing FX chain')
            self._chain_type = l
            if not self._backend:
                self.rescan_parents()
            else:
                self.maybe_initialize()
    
    ###########
    ## SLOTS
    ###########

    # Update mode from the back-end.
    @ShoopSlot(thread_protection = ThreadProtectionType.OtherThread)
    def updateOnOtherThread(self):
        if not self._initialized:
            return

        state = self._backend_object.get_state()
        self._new_ready = state.ready
        self._new_ui_visible = state.visible
        self._new_active = state.active
        self._n_pending_updates += 1
        
        self._signal_sender.do_emit()
    
    # Update on GUI thread
    @ShoopSlot()
    def updateOnGuiThread(self):
        if not self._initialized or not self.isValid():
            return
        if self._n_pending_updates == 0:
            return
        
        prev_active = self._active
        prev_ready = self._ready
        prev_ui_visible = self._ui_visible

        self._ready = self._new_ready
        self._ui_visible = self._new_ui_visible
        self._active = self._new_active

        self._n_pending_updates = 0

        if prev_ready != self._ready:
            self.logger.debug(lambda: f'ready -> {self._ready}')
            self.readyChanged.emit(self._ready)
        if prev_ui_visible != self._ui_visible:
            self.logger.debug(lambda: f'ui_visible -> {self._ui_visible}')
            self.uiVisibleChanged.emit(self._ui_visible)
        if prev_active != self._active:
            self.logger.debug(lambda: f'active -> {self._active}')
            self.activeChanged.emit(self._active)
    
    @ShoopSlot(result='QVariant')
    def get_backend(self):
        maybe_backend = findFirstParent(self, lambda p: p and isinstance(p, QQuickItem) and p.inherits('Backend'))
        if maybe_backend:
            return maybe_backend
        self.logger.throw_error("Could not find backend!")
    
    @ShoopSlot(result='QVariant')
    def get_backend_obj(self):
        return self._backend_object
    
    @ShoopSlot(result="QVariant")
    def get_state_str(self):
        if self._backend_object:
            rval = self._backend_object.get_state_str()
            return rval
        self.logger.throw_error("Getting internal state of uninitialized FX chain")
    
    @ShoopSlot(str)
    def restore_state(self, state_str):
        if self._backend_object:
            self._backend_object.restore_state(state_str)
        else:
            self.logger.throw_error("Restoring internal state of uninitialized FX chain")
    
    def maybe_initialize(self):
        if self._backend and self._backend.initialized and self._chain_type != None and not self._backend_object:
            self._backend_object = self._backend.get_backend_session_obj().create_fx_chain(FXChainType(self._chain_type), self._title)
            if self._backend_object:
                self._initialized = True
                self.set_active(self._active)
                self.set_ui_visible(self._ui_visible)
                self.update()
                self.initializedChanged.emit(True)