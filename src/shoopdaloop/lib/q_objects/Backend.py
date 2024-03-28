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
from ..backend_wrappers import open_driver_audio_port as backend_open_driver_audio_port, open_driver_midi_port as backend_open_driver_midi_port
from ..findChildItems import findChildItems
from .Logger import Logger

all_active_backend_objs = set()

# Wraps the back-end session + driver in a single object.
class Backend(ShoopQQuickItem):
    def __init__(self, parent=None):
        super(Backend, self).__init__(parent)
        self._update_interval_ms = 50
        self._timer = None
        self._timer_thread = QThread(self)
        self._timer_thread.start()
        self._initialized = False
        self._closed = False
        self._client_name_hint = None
        self._driver_type = None
        self._backend_session_obj = None
        self._backend_driver_obj = None
        self._backend_child_objects = set()
        self._driver_setting_overrides = None
        self._xruns = self._new_xruns = 0
        self._dsp_load = self._new_dsp_load = 0.0
        self._actual_driver_type = self._new_actual_driver_type = None
        self.destroyed.connect(self.close)
        self.logger = Logger()
        self.logger.name = "Frontend.Backend"
        self.lock = threading.Lock()
        self._last_processed = self._new_last_processed = 1
        self._n_updates_pending = 0
        
        self._signal_sender = ThreadUnsafeSignalEmitter()
        self._signal_sender.signal.connect(self.updateOnGuiThread, Qt.QueuedConnection)
        
        self.setObjectName("ShoopBackend")
        
        global all_active_backend_objs
        all_active_backend_objs.add(self)
    
    updated = ShoopSignal()

    ######################
    # PROPERTIES
    ######################

    updateIntervalMsChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=updateIntervalMsChanged)
    def update_interval_ms(self):
        return self._update_interval_ms
    @update_interval_ms.setter
    def update_interval_ms(self, u):
        if u != self._update_interval_ms:
            self._update_interval_ms = u
            self.init_timer()
    
    initializedChanged = ShoopSignal(bool)
    @ShoopProperty(bool, notify=initializedChanged)
    def initialized(self):
        return self._initialized
    
    actualBackendTypeChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=actualBackendTypeChanged)
    def actual_backend_type(self):
        return self._actual_driver_type if self._actual_driver_type != None else -1
    @actual_backend_type.setter
    def actual_backend_type(self, n):
        if self._actual_driver_type != n:
            self._actual_driver_type = n
            self.logger.instanceIdentifier = AudioDriverType(n).name
            self.actualBackendTypeChanged.emit(n)
    
    clientNameHintChanged = ShoopSignal(str)
    @ShoopProperty(str, notify=clientNameHintChanged)
    def client_name_hint(self):
        return (self._client_name_hint if self._client_name_hint else "")
    @client_name_hint.setter
    def client_name_hint(self, n):
        if self._initialized:
            self.logger.throw_error("Back-end client name hint may only be set once.")
        self._client_name_hint = n
        self.maybe_init()

    # TODO: rename
    backendTypeChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=backendTypeChanged)
    def backend_type(self):
        return (self._driver_type.value if self._driver_type else AudioDriverType.Dummy.value)
    @backend_type.setter
    def backend_type(self, n):
        if self._initialized:
            self.logger.throw_error("Back-end driver type can only be set once.")
        self._driver_type = AudioDriverType(n)
        self.maybe_init()
    
    xrunsChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=xrunsChanged)
    def xruns(self):
        return self._xruns
    @xruns.setter
    def xruns(self, n):
        if self._xruns != n:
            self._xruns = n
            self.xrunsChanged.emit(n)
    
    lastProcessedChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=lastProcessedChanged)
    def last_processed(self):
        return self._last_processed
    @last_processed.setter
    def last_processed(self, n):
        if self._last_processed != n:
            self._last_processed = n
            self.lastProcessedChanged.emit(n)
    
    dspLoadChanged = ShoopSignal(float)
    @ShoopProperty(float, notify=dspLoadChanged)
    def dsp_load(self):
        return self._dsp_load
    @dsp_load.setter
    def dsp_load(self, n):
        if self._dsp_load != n:
            self._dsp_load = n
            self.dspLoadChanged.emit(n)
    
    driverSettingOverridesChanged = ShoopSignal('QVariant')
    @ShoopProperty('QVariant', notify=driverSettingOverridesChanged)
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

    @ShoopSlot('QVariant')
    def registerBackendObject(self, obj):
        with self.lock:
            self._backend_child_objects.add(weakref.ref(obj))
    
    @ShoopSlot('QVariant')
    def unregisterBackendObject(self, obj):
        with self.lock:
            self._backend_child_objects.remove(weakref.ref(obj))

    @ShoopSlot()
    def updateOnGuiThread(self):
        self.logger.trace(lambda: f'update on GUI thread (# {self._n_updates_pending}, initialized {self._initialized})')
        if self._n_updates_pending == 0:
            return
        if not self._initialized or not self.isValid():
            return

        # Calling the setters will force emitting a changed signal
        # if necessary
        self.xruns = self._new_xruns
        self.dsp_load = self._new_dsp_load
        self.actual_backend_type = self._new_actual_backend_type
        self.last_processed = self._new_last_processed

        # Update other objects
        toRemove = []
        with self.lock:
            for obj in self._backend_child_objects:
                _obj = obj()
                if not _obj:
                    toRemove.append(obj)
                elif hasattr(_obj, 'updateOnGuiThread'):
                    _obj.updateOnGuiThread()
            for r in toRemove:
                self._backend_child_objects.remove(r)

        self._n_updates_pending = 0
        self.updated.emit()

    # Update data from the back-end. This function does not run in the GUI thread.
    @ShoopSlot(thread_protection=ThreadProtectionType.OtherThread)
    def updateOnOtherThread(self):
        self.logger.trace(lambda: f'update on back-end thread (initialized {self._initialized})')
        if not self._initialized:
            return
        driver_state = self._backend_driver_obj.get_state()
        
        # Set the Python state directly and queue an update on the GUI thread
        self._new_dsp_load = driver_state.dsp_load
        self._new_xruns = min(2**31-1, self._xruns + driver_state.xruns)
        self._new_actual_backend_type = self._driver_type.value
        self._new_last_processed = driver_state.last_processed
        self._n_updates_pending += 1
        
        # Some objects have an update function which can run outside of the
        # GUI thread, allowing them to continue working if the GUI thread
        # hangs. Trigger those updates here.
        with self.lock:
            for obj in self._backend_child_objects:
                _obj = obj()
                if _obj and hasattr(_obj, 'updateOnOtherThread'):
                    _obj.updateOnOtherThread()
        
        self._signal_sender.do_emit()
    
    @ShoopSlot(result=int)
    def get_sample_rate(self):
        if not self._backend_driver_obj:
            self.logger.warning("Attempting to get sample rate before back-end initialized.")
            import traceback
            traceback.print_stack()
            return 1
        return self._backend_driver_obj.get_sample_rate()
    
    @ShoopSlot(result=int)
    def get_buffer_size(self):
        return self._backend_driver_obj.get_buffer_size()

    @ShoopSlot()
    def close(self):
        if self._closed:
            return
        self.logger.debug(lambda: "Closing")
        QMetaObject.invokeMethod(
            self._timer,
            'stop'
        )
        while self._timer.isActive():
            time.sleep(0.005)
        if Shiboken.isValid(self._timer_thread):
            self._timer_thread.exit()
            while self._timer_thread.isRunning():
                time.sleep(0.005)
        if self._initialized:
            self._backend_session_obj.destroy()
            self._backend_driver_obj.destroy()
        self._initialized = False
        self._closed = True
    
    # Get the wrapped back-end object.
    @ShoopSlot(result='QVariant')
    def get_backend_driver_obj(self):
        return self._backend_driver_obj
    
    @ShoopSlot(result='QVariant')
    def get_backend_session_obj(self):
        return self._backend_session_obj
    
    @ShoopSlot()
    def dummy_enter_controlled_mode(self):
        self._backend_driver_obj.dummy_enter_controlled_mode()
    
    @ShoopSlot()
    def dummy_enter_automatic_mode(self):
        self._backend_driver_obj.dummy_enter_automatic_mode()
    
    @ShoopSlot(result=bool)
    def dummy_is_controlled(self):
        self._backend_driver_obj.dummy_is_controlled()
    
    @ShoopSlot(int, thread_protection=ThreadProtectionType.AnyThread)
    def dummy_request_controlled_frames(self, n):
        self._backend_driver_obj.dummy_request_controlled_frames(n)
        
    @ShoopSlot(result=int)
    def dummy_n_requested_frames(self):
        return self._backend_driver_obj.dummy_n_requested_frames()

    @ShoopSlot()
    def dummy_run_requested_frames(self):
        self._backend_driver_obj.dummy_run_requested_frames()
    
    @ShoopSlot(str, int, int)
    def dummy_add_external_mock_port(self, name, direction, data_type):
        return self._backend_driver_obj.dummy_add_external_mock_port(name, direction, data_type)
    
    @ShoopSlot(str)
    def dummy_remove_external_mock_port(self, name):
        return self._backend_driver_obj.dummy_remove_external_mock_port(name)
    
    @ShoopSlot()
    def dummy_remove_all_external_mock_ports(self):
        return self._backend_driver_obj.dummy_remove_all_external_mock_ports()
    
    @ShoopSlot(thread_protection = False)
    def wait_process(self):
        self._backend_driver_obj.wait_process()
    
    @ShoopSlot()
    def maybe_init(self):
        if not self._initialized and \
           not self._closed and \
           self._client_name_hint != None and \
           self._driver_type != None and \
           self._driver_setting_overrides != None:
            self.logger.debug(lambda: "Initializing")
            self.init()
        elif self._closed:
            self.logger.trace(lambda: "Already closed")
        elif self._initialized:
            self.logger.trace(lambda: "Already initialized")
        else:
            self.logger.debug(lambda: "Not initializing yet")
    
    @ShoopSlot(result='QVariant')
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

    @ShoopSlot(int, result=bool)
    def backend_type_is_supported(self, type):
        return audio_driver_type_supported(AudioDriverType(type))
    
    @ShoopSlot(str, int, result='QVariant')
    def open_driver_audio_port(self, name_hint, direction, min_n_ringbuffer_samples):
        return backend_open_driver_audio_port(self._backend_session_obj, self._backend_driver_obj, name_hint, direction, min_n_ringbuffer_samples)
    
    @ShoopSlot(str, int, result='QVariant')
    def open_driver_midi_port(self, name_hint, direction, min_n_ringbuffer_samples):
        return backend_open_driver_midi_port(self._backend_session_obj, self._backend_driver_obj, name_hint, direction, min_n_ringbuffer_samples)

    @ShoopSlot()
    def segfault_on_process_thread(self):
        if (self._backend_session_obj):
            self._backend_session_obj.segfault_on_process_thread()
    
    @ShoopSlot()
    def abort_on_process_thread(self):
        if (self._backend_session_obj):
            self._backend_session_obj.abort_on_process_thread()
    
    @ShoopSlot(str, int, int)
    def find_external_ports(self, maybe_name_regex, port_direction, data_type):
        return self._backend_driver_obj.find_external_ports(maybe_name_regex, port_direction, data_type)
    
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
        
        self.logger.debug(lambda: "Initialized")
        self._initialized = True
        self.initializedChanged.emit(True)
        self.init_timer()
    
    def init_timer(self):
        if not self._timer:
            self._timer = QTimer()
            self._timer.setSingleShot(False)
            self._timer.moveToThread(self._timer_thread)
            self._timer.timeout.connect(self.updateOnOtherThread, Qt.DirectConnection)
        QMetaObject.invokeMethod(
            self._timer,
            'start',
            Q_ARG(int, self._update_interval_ms)
        )
    
def close_all_backends():
    global all_active_backend_objs
    a = copy.copy(all_active_backend_objs)
    for b in a:
        b.close()