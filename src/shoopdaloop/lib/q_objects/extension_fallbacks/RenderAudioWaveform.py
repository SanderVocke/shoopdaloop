from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQuick import QQuickItem

class RenderAudioWaveform(QQuickItem):
    def __init__(self, parent=None):
        super(RenderAudioWaveform, self).__init__(parent)
    
    inputDataChanged = Signal('QVariant')
    @Property('QVariant', notify=inputDataChanged)
    def input_data(self):
        return []
    @input_data.setter
    def input_data(self, v):
        pass

    samplesPerBinChanged = Signal(float)
    @Property(float, notify=samplesPerBinChanged)
    def samples_per_bin(self):
        return 0.0
    @samples_per_bin.setter
    def samples_per_bin(self, v):
        pass
    
    samplesOffsetChanged = Signal(int)
    @Property(int, notify=samplesOffsetChanged)
    def samples_offset(self):
        return 0
    @samples_offset.setter
    def samples_offset(self, v):
        pass
    
    @Slot()
    def preprocess(self):
        pass

    @Slot()
    def update_lines(self):
        pass