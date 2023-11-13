from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQuick import QQuickItem

from ..logging import Logger

logger = Logger("Frontend.RenderAudioWaveform")

# Fallbacks
def render(painter, preproc_data, width, height, samples_per_bin, samples_offset):
    pass
def update(preproc_data, n_samples, input_data):
    pass
def create_preproc():
    return True
def destroy_preproc(preproc):
    pass

# Try to load our back-end rendering extension.
try:
    import qt_extensions
    render = qt_extensions.render_audio_waveform
    update = qt_extensions.update_preprocessing_data
    create_preproc = qt_extensions.create_audio_waveform_preprocessing_data
    destroy_preproc = qt_extensions.destroy_audio_waveform_preprocessing_data
except Exception as e:
    logger.warning("Unable to load back-end extension for rendering audio waveforms. Waveforms will not be visible. Error: {}".format(e))

class RenderAudioWaveform(QQuickItem):
    def __init__(self, parent=None):
        super(RenderAudioWaveform, self).__init__(parent)
        self.preproc_data = create_preproc()
        self._input_data = []
        self._samples_per_bin = 1.0
    
    inputDataChanged = Signal('QVariant')
    @Property('QVariant', notify=inputDataChanged)
    def input_data(self):
        return self._input_data
    @input_data.setter
    def input_data(self, v):
        pass

    samplesPerBinChanged = Signal(float)
    @Property(float, notify=samplesPerBinChanged)
    def samples_per_bin(self):
        return self._samples_per_bin
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

    def __del__(self):
        if self.preproc_data:
            destroy_preproc(self.preproc_data)
            self.preproc_data = None