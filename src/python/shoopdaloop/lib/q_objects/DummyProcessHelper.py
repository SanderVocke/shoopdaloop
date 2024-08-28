from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQuick import QQuickItem

from .ShoopPyObject import *

from ..backend_wrappers import *
from ..logging import Logger

import threading

# This object exists only for helping tests which test scenarios
# during which the GUI is purposefully frozen.
# This class can trigger processing of samples in a controlled
# dummy back-end without needing anything from the GUI thread.
class DummyProcessHelper(ShoopQQuickItem):
    def __init__(self, parent=None):
        super(DummyProcessHelper, self).__init__(parent)
        self._wait_start = 0.0
        self._n_iters = 0
        self._samples_per_iter = 0
        self._wait_interval = 0.0
        self._backend = None
        self._active = False
        self.logger = Logger('Frontend.DummyProcessHelper')

    
    ##############
    ## PROPERTIES
    ##############
        
    # wait_start
    waitStartChanged = ShoopSignal(float)
    @ShoopProperty(float, notify=waitStartChanged)
    def wait_start(self):
        return self._wait_start
    @wait_start.setter
    def wait_start(self, val):
        self._wait_start = val
        self.waitStartChanged.emit(self._wait_start)
    
    # wait_interval
    waitIntervalChanged = ShoopSignal(float)
    @ShoopProperty(float, notify=waitIntervalChanged)
    def wait_interval(self):
        return self._wait_interval
    @wait_interval.setter
    def wait_interval(self, val):
        self._wait_interval = val
        self.waitIntervalChanged.emit(self._wait_interval)
    
    # n_iters
    nItersChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=nItersChanged)
    def n_iters(self):
        return self._n_iters
    @n_iters.setter
    def n_iters(self, val):
        self._n_iters = val
        self.nItersChanged.emit(self._n_iters)
    
    # samples_per_iter
    samplesPerIterChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=samplesPerIterChanged)
    def samples_per_iter(self):
        return self._samples_per_iter
    @samples_per_iter.setter
    def samples_per_iter(self, val):
        self._samples_per_iter = val
        self.samplesPerIterChanged.emit(self._samples_per_iter)
    
    # backend
    backendChanged = ShoopSignal('QVariant')
    @ShoopProperty('QVariant', notify=backendChanged)
    def backend(self):
        return self._backend
    @backend.setter
    def backend(self, val):
        self._backend= val
        self.backendChanged.emit(self._backend)
    
    # active
    activeChanged = ShoopSignal(bool)
    @ShoopProperty(bool, notify=activeChanged)
    def active(self):
        return self._active
    @active.setter
    def active(self, v):
        if v != self._active:
            self._active = v
            self.activeChanged.emit(v)

    #################
    ## SLOTS
    #################
        
    @ShoopSlot()
    def start(self):
        if self.active:
            raise Exception('Cannot start dummy process helper: still running')
        
        self.active = True
        def fn(samples_per_iter=self._samples_per_iter, backend=self._backend, wait_start=self._wait_start, wait_interval=self._wait_interval, n_iters=self._n_iters):
            time.sleep(wait_start)
            for i in range(n_iters):
                self.logger.debug('trigger')
                backend.wait_process()
                backend.dummy_request_controlled_frames(samples_per_iter)
                if i < (n_iters-1):
                    time.sleep(wait_interval)
            backend.wait_process()
            self.active = False
        
        self._thread = threading.Thread(target=fn)
        self._thread.daemon = True

        self.logger.debug('start')
        self._thread.start()