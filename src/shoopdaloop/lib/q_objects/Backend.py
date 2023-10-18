import re
import time
import os
import tempfile
import json
import sys
import weakref

from PySide6.QtCore import Qt, QObject, Signal, Property, Slot, QTimer
from PySide6.QtQuick import QQuickItem

from ..backend_wrappers import *
from ..findChildItems import findChildItems
from ..logging import Logger

# Wraps the back-end.
class Backend(QQuickItem):
    def __init__(self, parent=None):
        super(Backend, self).__init__(parent)
        self._update_interval_ms = 50
        self._timer = None
        self._initialized = False
        self._client_name_hint = None
        self._backend_type = None
        self._backend_argstring = None
        self._backend_obj = None
        self._backend_child_objects = set()
        self._xruns = 0
        self._dsp_load = 0.0
        self._actual_backend_type = None
        self.destroyed.connect(self.close)
        self.logger = Logger('Frontend.Backend')
    
    update = Signal()

    ######################
    # PROPERTIES
    ######################

    updateIntervalMsChanged = Signal(int)
    @Property(int, notify=updateIntervalMsChanged)
    def update_interval_ms(self):
        return self._update_interval_ms
    @update_interval_ms.setter
    def update_interval_ms(self, u):
        if u != self._update_interval_ms:
            self._update_interval_ms = u
            self.init_timer()
    
    initializedChanged = Signal(bool)
    @Property(bool, notify=initializedChanged)
    def initialized(self):
        return self._initialized
    
    actualBackendTypeChanged = Signal(int)
    @Property(int, notify=actualBackendTypeChanged)
    def actual_backend_type(self):
        return self._actual_backend_type if self._actual_backend_type != None else -1
    @actual_backend_type.setter
    def actual_backend_type(self, n):
        if self._actual_backend_type != n:
            self._actual_backend_type = n
            self.actualBackendTypeChanged.emit(n)
    
    clientNameHintChanged = Signal(str)
    @Property(str, notify=clientNameHintChanged)
    def client_name_hint(self):
        return (self._client_name_hint if self._client_name_hint else "")
    @client_name_hint.setter
    def client_name_hint(self, n):
        if self._initialized:
            self.logger.throw_error("Back-end client name hint may only be set once.")
        self._client_name_hint = n
        self.maybe_init()

    backendTypeChanged = Signal(int)
    @Property(int, notify=backendTypeChanged)
    def backend_type(self):
        return (self._backend_type.value if self._backend_type else BackendType.Dummy.value)
    @backend_type.setter
    def backend_type(self, n):
        if self._initialized:
            self.logger.throw_error("Back-end type can only be set once.")
        self._backend_type = BackendType(n)
        self.maybe_init()
    
    backendArgstringChanged = Signal(str)
    @Property(str, notify=backendArgstringChanged)
    def backend_argstring(self):
        return self._backend_argstring
    @backend_argstring.setter
    def backend_argstring(self, n):
        if self._initialized:
            self.logger.throw_error("Back-end argstring can only be set once.")
        self._backend_argstring = n
        self.maybe_init()
    
    xrunsChanged = Signal(int)
    @Property(int, notify=xrunsChanged)
    def xruns(self):
        return self._xruns
    @xruns.setter
    def xruns(self, n):
        if self._xruns != n:
            self._xruns = n
            self.xrunsChanged.emit(n)
    
    dspLoadChanged = Signal(float)
    @Property(float, notify=dspLoadChanged)
    def dsp_load(self):
        return self._dsp_load
    @dsp_load.setter
    def dsp_load(self, n):
        if self._dsp_load != n:
            self._dsp_load = n
            self.dspLoadChanged.emit(n)
    
    ###########
    ## SLOTS
    ###########

    @Slot('QVariant')
    def registerBackendObject(self, obj):
        self._backend_child_objects.add(weakref.ref(obj))

    @Slot()
    def doUpdate(self):
        if not self.initialized:
            return
        state = self._backend_obj.get_state()
        self.dsp_load = state.dsp_load_percent
        self.xruns += state.xruns_since_last
        self.actual_backend_type = state.actual_type.value
        
        toRemove = []
        for obj in self._backend_child_objects:
            _obj = obj()
            if not _obj:
                toRemove.append(obj)
            else:
                _obj.update()
        for r in toRemove:
            self._backend_child_objects.remove(r)
        self.update.emit()
    
    @Slot(result=int)
    def get_sample_rate(self):
        return self._backend_obj.get_sample_rate()
    
    @Slot(result=int)
    def get_buffer_size(self):
        return self._backend_obj.get_buffer_size()

    @Slot()
    def close(self):
        self.logger.info(lambda: "Closing back-end")
        if self._initialized:
            self._backend_obj.terminate()
        self._initialized = False
        self.logger.info(lambda: "Back-end closed")
    
    # Get the wrapped back-end object.
    @Slot(result='QVariant')
    def get_backend_obj(self):
        return self._backend_obj
    
    @Slot()
    def dummy_enter_controlled_mode(self):
        self._backend_obj.dummy_enter_controlled_mode()
    
    @Slot()
    def dummy_enter_automatic_mode(self):
        self._backend_obj.dummy_enter_automatic_mode()
    
    @Slot(result=bool)
    def dummy_is_controlled(self):
        self._backend_obj.dummy_is_controlled()
    
    @Slot(int)
    def dummy_request_controlled_frames(self, n):
        self._backend_obj.dummy_request_controlled_frames(n)
        
    @Slot(result=int)
    def dummy_n_requested_frames(self):
        return self._backend_obj.dummy_n_requested_frames()
    
    @Slot()
    def dummy_wait_process(self):
        self._backend_obj.dummy_wait_process()
    
    @Slot()
    def maybe_init(self):
        if not self._initialized and self._client_name_hint != None and self._backend_type != None and self._backend_argstring != None:
            self.init()
    
    @Slot(result='QVariant')
    def get_profiling_report(self):
        def report_item_to_dict(item):
            return {
                'name': item.key,
                'worst': item.worst,
                'most_recent': item.most_recent,
                'average': item.average,
                'n_samples': item.n_samples,
            }

        if (self._backend_obj):
            report = self._backend_obj.get_profiling_report()
            dct = {}
            for item in report.items:
                dct[item.key] = report_item_to_dict(item)
            return dct
        return []
    
    ################
    ## INTERNAL METHODS
    ################

    def init(self):
        self.logger.debug(lambda: "Initializing with type {}, client name hint {}, argstring {}".format(self._backend_type, self._client_name_hint, self._backend_argstring))
        if self._initialized:
            self.logger.throw_error("May not initialize more than one back-end at a time.")
        self._backend_obj = init_backend(self._backend_type, self._client_name_hint, self._backend_argstring)
        if not self._backend_obj:
            self.logger.throw_error("Failed to initialize back-end.")
        self._initialized = True
        self.initializedChanged.emit(True)
        self.init_timer()
    
    def init_timer(self):
        if self._timer:
            self._timer.active = False
            self._timer = None
        self._timer = QTimer(self)
        self._timer.setSingleShot(False)
        self._timer.timeout.connect(self.doUpdate)
        self._timer.start(self._update_interval_ms)
    