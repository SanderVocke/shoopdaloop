from SLLooperManager import SLLooperManager

class SLLooperManagerWithFX(SLLooperManager):

    # State change notifications
    slWetLooperIndexChanged = pyqtSignal(int)

    def __init__(self, parent=None, sl_dry_looper_index=0, sl_wet_looper_index=0):
        super(SLLooperManagerWithFX, self).__init__(parent, sl_dry_looper_index)

    #######################
    # PROPERTIES
    #######################

    @pyqtProperty(int, notify=slWetLooperIndexChanged)
    def sl_wet_looper_index(self):
        return self._sl_wet_looper_index

    @sl_wet_looper_index.setter
    def sl_wet_looper_index(self, i):
        if self._sl_wet_looper_index != i:
            self._sl_wet_looper_index = i
            self.slWetLooperIndexChanged.emit(i)
            self.updateConnected()
            self.start_sync()