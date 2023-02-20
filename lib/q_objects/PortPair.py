from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot

from lib.q_objects.BackendLoopAudioChannel import BackendLoopAudioChannel
from lib.q_objects.BackendLoopMidiChannel import BackendLoopMidiChannel
from lib.backend import open_audio_port, open_midi_port, PortDirection

class PortPair(QObject):
    def __init__(self, input_port, output_port, parent=None):
        super(PortPair, self).__init__(parent)
        self._input_port = input_port
        self._output_port = output_port
        self._port_type = 'unknown'
    
    @classmethod
    def create_audio(self, input_name_hint, output_name_hint):
        self._input_port = open_audio_port(input_name_hint, PortDirection.Input)
        self._output_port = open_audio_port(output_name_hint, PortDirection.Output)
        self._input_port.parent = self
        self._output_port.parent = self
        self._port_type = 'audio'
    
    @classmethod
    def create_midi(self, input_name_hint, output_name_hint):
        self._input_port = open_midi_port(input_name_hint, PortDirection.Input)
        self._output_port = open_midi_port(output_name_hint, PortDirection.Output)
        self._input_port.parent = self
        self._output_port.parent = self
        self._port_type = 'midi'
    
    portTypeChanged = pyqtSignal(str)
    @pyqtProperty(str, notify=portTypeChanged)
    def port_type(self):
        return self._port_type
    
    inputPortChanged = pyqtSignal('QVariant')
    @pyqtProperty('QVariant', notify=inputPortChanged)
    def input_port(self):
        return self._input_port
    
    outputPortChanged = pyqtSignal('QVariant')
    @pyqtProperty('QVariant', notify=outputPortChanged)
    def output_port(self):
        return self._output_port
