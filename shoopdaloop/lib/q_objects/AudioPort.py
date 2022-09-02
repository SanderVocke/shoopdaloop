import re
import time
import os
import tempfile
import json
from typing import *
import sys

from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQuick import QQuickItem

from .Port import Port

from ..backend_wrappers import PortDirection
from ..findFirstParent import findFirstParent
from ..findChildItems import findChildItems

# Wraps a back-end port.
class AudioPort(Port):
    def __init__(self, parent=None):
        super(AudioPort, self).__init__(parent)
        self._peak = 0.0
        self._volume = 1.0
        self._passthrough_volume = 1.0

    ######################
    # PROPERTIES
    ######################

    # peak
    peakChanged = Signal(float)
    @Property(float, notify=peakChanged)
    def peak(self):
        return self._peak
    @peak.setter
    def peak(self, s):
        if self._peak != s:
            self._peak = s
            self.peakChanged.emit(s)
    
    # volume
    volumeChanged = Signal(float)
    @Property(float, notify=volumeChanged)
    def volume(self):
        return self._volume
    @volume.setter
    def volume(self, s):
        if self._volume != s:
            self._volume = s
            self.volumeChanged.emit(s)
    
    # passthrough volume
    passthroughVolumeChanged = Signal(float)
    @Property(float, notify=passthroughVolumeChanged)
    def passthrough_volume(self):
        return self._passthrough_volume
    @passthrough_volume.setter
    def passthrough_volume(self, s):
        if self._passthrough_volume != s:
            self._passthrough_volume = s
            self.passthroughVolumeChanged.emit(s)
    
    ###########
    ## SLOTS
    ###########

    # Update mode from the back-end.
    @Slot()
    def update(self):
        if not self.initialized:
            return
        state = self._backend_obj.get_state()
        self.peak = state.peak
        self.name = state.name

        self.volume = state.volume
        self.passthrough_volume = state.passthrough_volume
        self.muted = state.muted
        self.passthrough_muted = state.passthrough_muted
    
    @Slot(float)
    def set_volume(self, volume):
        if self._backend_obj:
            self._backend_obj.set_volume(volume)
    
    @Slot(float)
    def set_passthrough_volume(self, passthrough_volume):
        if self._backend_obj:
            self._backend_obj.set_passthrough_volume(passthrough_volume)

    ##########
    ## INTERNAL MEMBERS
    ##########    
    def maybe_initialize_internal(self, name_hint, direction):
        # Internal ports are owned by FX chains.
        maybe_fx_chain = findFirstParent(self, lambda p: p and isinstance(p, QQuickItem) and p.inherits('FXChain'))
        if maybe_fx_chain and not self._backend_obj:
            if not maybe_fx_chain.initialized:
                maybe_fx_chain.initializedChanged.connect(lambda: self.maybe_initialize())
            else:
                # Determine our index in the FX chain
                def find_index():
                    idx = 0
                    for port in findChildItems(maybe_fx_chain, lambda i: i.inherits('AudioPort')):
                        if port == self:
                            return idx
                        elif port.direction == self.direction:
                            idx += 1
                    return None
                idx = find_index()
                if idx == None:
                    raise Exception('Could not find self in FX chain')
                # Now request our backend object.
                if self.direction == PortDirection.Input.value:
                    self._backend_obj = self.backend.get_backend_obj().get_fx_chain_audio_output_port(
                        maybe_fx_chain.get_backend_obj(),
                        idx
                    )
                else:
                    self._backend_obj = self.backend.get_backend_obj().get_fx_chain_audio_input_port(
                        maybe_fx_chain.get_backend_obj(),
                        idx
                    )
                self.push_state()

    def maybe_initialize_external(self, name_hint, direction):
        self._backend_obj = self.backend.get_backend_obj().open_jack_audio_port(name_hint, direction)
        self.push_state()

    def push_state(self):
        self.set_muted(self.muted)
        self.set_passthrough_muted(self.passthrough_muted)
        self.set_passthrough_volume(self.passthrough_volume)
        self.set_volume(self.volume)
        
    def maybe_initialize_impl(self, name_hint, direction, is_internal):
        if is_internal:
            self.maybe_initialize_internal(name_hint, direction)
        else:
            self.maybe_initialize_external(name_hint, direction)