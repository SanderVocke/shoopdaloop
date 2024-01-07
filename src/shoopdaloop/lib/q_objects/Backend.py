import re
import time
import os
import tempfile
import json
import sys
import weakref
import traceback
import json
from threading import Lock

from PySide6.QtCore import Qt, Signal, Property, Slot, QTimer, QThread, QMetaObject, Q_ARG, QEvent
from PySide6.QtQml import QJSValue

from .ShoopPyObject import *

from ..backend_wrappers import *
from ..backend_wrappers import open_audio_port as backend_open_audio_port, open_midi_port as backend_open_midi_port
from ..findChildItems import findChildItems
from .Logger import Logger

# Wraps the back-end session + driver in a single object.
class Backend(ShoopQQuickItem):
    def __init__(self, parent=None):
        super(Backend, self).__init__(parent)
        self._update_interval_ms = 50
        self._timer = None
        self._timer_thread = QThread(self)
        self._timer_thread.start()
        self._initialized = False
        self._client_name_hint = None
        self._driver_type = None
        self._backend_session_obj = None
        self._backend_driver_obj = None
        self._backend_child_objects = set()
        self._driver_setting_overrides = None
        self._xruns = 0
        self._dsp_load = 0.0
        self._actual_driver_type = None
        self.destroyed.connect(self.close)
        self.logger = Logger()
        self.logger.name = "Frontend.Backend"
        self.lock = threading.Lock()
        self._last_processed = 1
    
    update = Signal()
    updated = Signal()

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
        return self._actual_driver_type if self._actual_driver_type != None else -1
    @actual_backend_type.setter
    def actual_backend_type(self, n):
        if self._actual_driver_type != n:
            self._actual_driver_type = n
            self.logger.instanceIdentifier = AudioDriverType(n).name
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

    # TODO: rename
    backendTypeChanged = Signal(int)
    @Property(int, notify=backendTypeChanged)
    def backend_type(self):
        return (self._driver_type.value if self._driver_type else AudioDriverType.Dummy.value)
    @backend_type.setter
    def backend_type(self, n):
        if self._initialized:
            self.logger.throw_error("Back-end driver type can only be set once.")
        self._driver_type = AudioDriverType(n)
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
    
    lastProcessedChanged = Signal(int)
    @Property(int, notify=lastProcessedChanged)
    def last_processed(self):
        return self._last_processed
    @last_processed.setter
    def last_processed(self, n):
        if self._last_processed != n:
            self._last_processed = n
            self.lastProcessedChanged.emit(n)
    
    dspLoadChanged = Signal(float)
    @Property(float, notify=dspLoadChanged)
    def dsp_load(self):
        return self._dsp_load
    @dsp_load.setter
    def dsp_load(self, n):
        if self._dsp_load != n:
            self._dsp_load = n
            self.dspLoadChanged.emit(n)
    
    driverSettingOverridesChanged = Signal('QVariant')
    @Property('QVariant', notify=driverSettingOverridesChanged)
    def driver_setting_overrides(self):
        return self._driver_setting_overrides
    @driver_setting_overrides.setter
    def driver_setting_overrides(self, n):
        if isinstance(n, QJSValue):
            n = n.toVariant()
        if n != self._driver_setting_overrides:
            if self._initialized:
                self.logger.throw_error("Back-end driver settings cannot be changed once driver is started.")
            self._driver_setting_overrides = n
            self.driverSettingOverridesChanged.emit(n)
            self.maybe_init()
    
    ###########
    ## SLOTS
    ###########

    @Slot('QVariant')
    def registerBackendObject(self, obj):
        with self.lock:
            self._backend_child_objects.add(weakref.ref(obj))
    
    @Slot('QVariant')
    def unregisterBackendObject(self, obj):
        with self.lock:
            self._backend_child_objects.remove(weakref.ref(obj))

    @Slot()
    def doUpdate(self):
        self.logger.trace(lambda: 'update')
        if not self.initialized:
            return
        driver_state = self._backend_driver_obj.get_state()
        self.dsp_load = driver_state.dsp_load
        self.xruns = min(2**31-1, self.xruns + driver_state.xruns)
        self.actual_backend_type = self._driver_type.value
        self.last_processed = driver_state.last_processed
        
        toRemove = []
        with self.lock:
            for obj in self._backend_child_objects:
                _obj = obj()
                if not _obj:
                    toRemove.append(obj)
                else:
                    _obj.update()
            for r in toRemove:
                self._backend_child_objects.remove(r)
        self.update.emit()
        self.updated.emit()
    
    @Slot(result=int)
    def get_sample_rate(self):
        return self._backend_driver_obj.get_sample_rate()
    
    @Slot(result=int)
    def get_buffer_size(self):
        return self._backend_driver_obj.get_buffer_size()

    @Slot()
    def close(self):
        self.logger.debug(lambda: "Closing")
        QMetaObject.invokeMethod(
            self._timer,
            'stop'
        )
        while self._timer.isActive():
            time.sleep(0.005)
        self._timer_thread.exit()
        while self._timer_thread.isRunning():
            time.sleep(0.005)
        if self._initialized:
            self._backend_session_obj.destroy()
            self._backend_driver_obj.destroy()
        self._initialized = False
    
    # Get the wrapped back-end object.
    @Slot(result='QVariant')
    def get_backend_driver_obj(self):
        return self._backend_driver_obj
    
    @Slot(result='QVariant')
    def get_backend_session_obj(self):
        return self._backend_session_obj
    
    @Slot()
    def dummy_enter_controlled_mode(self):
        self._backend_driver_obj.dummy_enter_controlled_mode()
    
    @Slot()
    def dummy_enter_automatic_mode(self):
        self._backend_driver_obj.dummy_enter_automatic_mode()
    
    @Slot(result=bool)
    def dummy_is_controlled(self):
        self._backend_driver_obj.dummy_is_controlled()
    
    @Slot(int)
    def dummy_request_controlled_frames(self, n):
        self._backend_driver_obj.dummy_request_controlled_frames(n)
        
    @Slot(result=int)
    def dummy_n_requested_frames(self):
        return self._backend_driver_obj.dummy_n_requested_frames()
    
    @Slot()
    def wait_process(self):
        self._backend_driver_obj.wait_process()
    
    @Slot()
    def maybe_init(self):
        if not self._initialized and \
           self._client_name_hint != None and \
           self._driver_type != None and \
           self._driver_setting_overrides != None:
            self.logger.debug(lambda: "Initializing")
            self.init()
        else:
            self.logger.debug(lambda: "Not initializing yet")
    
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

        if (self._backend_session_obj):
            report = self._backend_session_obj.get_profiling_report()
            dct = {}
            for item in report.items:
                dct[item.key] = report_item_to_dict(item)
            return dct
        return []

    @Slot(int, result=bool)
    def backend_type_is_supported(self, type):
        return audio_driver_type_supported(AudioDriverType(type))
    
    @Slot(str, int, result='QVariant')
    def open_audio_port(self, name_hint, direction):
        return backend_open_audio_port(self._backend_session_obj, self._backend_driver_obj, name_hint, direction)
    
    @Slot(str, int, result='QVariant')
    def open_midi_port(self, name_hint, direction):
        return backend_open_midi_port(self._backend_session_obj, self._backend_driver_obj, name_hint, direction)
    
    ################
    ## INTERNAL METHODS
    ################

    def init(self):
        self.logger.debug(lambda: "Initializing with type {}, settings {}".format(self._driver_type, json.dumps(self._driver_setting_overrides)))
        if self._initialized:
            self.logger.throw_error("May not initialize more than one back-end at a time.")
        self._backend_session_obj = BackendSession.create()
        self._backend_driver_obj = AudioDriver.create(self._driver_type)
        
        if not self._backend_session_obj:
            self.logger.throw_error("Failed to initialize back-end session.")
        if not self._backend_driver_obj:
            self.logger.throw_error("Failed to initialize back-end driver.")

        if self._driver_type == AudioDriverType.Dummy:
            sample_rate = (self._driver_setting_overrides['sample_rate'] if 'sample_rate' in self._driver_setting_overrides else 48000)
            buffer_size = (self._driver_setting_overrides['buffer_size'] if 'buffer_size' in self._driver_setting_overrides else 256)
            settings = DummyAudioDriverSettings(
                client_name=self._client_name_hint,
                sample_rate=sample_rate,
                buffer_size=buffer_size
            )
            self._backend_driver_obj.start_dummy(settings)
        elif self._driver_type in [AudioDriverType.Jack, AudioDriverType.JackTest]:
            maybe_server_name = (self._driver_setting_overrides['jack_server'] if 'jack_server' in self._driver_setting_overrides else '')
            settings = JackAudioDriverSettings(
                client_name_hint = self._client_name_hint,
                maybe_server_name = maybe_server_name
            )
            self._backend_driver_obj.start_jack(settings)
        else:
            raise Exception("Unsupported back-end driver type.")
        
        self._backend_session_obj.set_audio_driver(self._backend_driver_obj)
        self._backend_driver_obj.wait_process()
        self._backend_driver_obj.get_state() # TODO: this has implicit side-effect

        if not self._backend_driver_obj.active():
            raise Exception("Failed to initialize back-end driver.")
        
        self._initialized = True
        self.initializedChanged.emit(True)
        self.init_timer()
    
    def init_timer(self):
        if not self._timer:
            self._timer = QTimer()
            self._timer.setSingleShot(False)
            self._timer.moveToThread(self._timer_thread)
            self._timer.timeout.connect(self.doUpdate, Qt.DirectConnection)
        QMetaObject.invokeMethod(
            self._timer,
            'start',
            Q_ARG(int, self._update_interval_ms)
        )
    