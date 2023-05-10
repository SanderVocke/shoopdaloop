from PySide6.QtCore import Qt, QObject, QMetaObject, Q_ARG, Signal, Property, Slot, QTimer, QByteArray, QSize, QBuffer, QIODeviceBase
from PySide6.QtGui import QImage
from PySide6.QtQuick import QQuickItem

# Serialize an array of floats into a 8192-wide QImage as a QByteArray,
# which can be used in QML as an Image source.
class FloatsToImageString(QQuickItem):
    def __init__(self, parent=None):
        super(FloatsToImageString, self).__init__(parent)
        self._input = None
        self._output = None
    
    # input
    inputChanged = Signal('QVariant')
    @Property('QVariant', notify=inputChanged)
    def input(self):
        return self._input
    @input.setter
    def input(self, l):
        print('set in: {}'.format(l))
        if l != self._input:
            self._input = l
            self.inputChanged.emit(l)
            self.update_output()                
    
    # output
    outputChanged = Signal('QVariant')
    @Property('QVariant', notify=outputChanged)
    def output(self):
        return self._output
    
    def update_output():
        prev = self._output

        if self._input:
            img = QImage (
                QSize(
                    8192,
                    (len(self._input) / 8192) + 1
                ),
                QImage.Format.Format_Grayscale8
            )
            print("img {}".format(img))
            for idx,f in enumerate(self._input):
                img.setPixel(
                    idx % 8192,
                    idx / 8192,
                    int(f * 127.0 + 128.0) # map -1..1 to 0..256
                )
            arr = QByteArray()
            print("arr {}".format(arr))
            buf = QBuffer(arr)
            print("buf {}".format(buf))
            buf.open(QIODeviceBase.OpenModeFlag.WriteOnly)
            img.save(buf, 'JPEG')
            st = 'data:image/jpg;base64,' + buffer.toBase64().data()
            self._output = st

        else:
            self._output = None        
        
        if self._output != prev:
            self.outputChanged.emit(self._output)