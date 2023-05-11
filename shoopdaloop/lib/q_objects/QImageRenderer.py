from PySide6.QtQuick import QQuickPaintedItem
from PySide6.QtGui import QPainter, qRgba
from PySide6.QtCore import QPoint, Signal, Property, Slot

class QImageRenderer(QQuickPaintedItem):
    def __init__(self, parent=None):
        super(QImageRenderer, self).__init__(parent)
        self._image = None
        print("inited")
    
    # image
    imageChanged = Signal('QVariant')
    @Property('QVariant', notify=imageChanged)
    def image(self):
        return self._image
    @image.setter
    def image(self, l):
        if l != self._image:
            self._image = l
            self.imageChanged.emit(l)
            self.imageWidthChanged.emit(l.width())
            self.imageHeightChanged.emit(l.height())
    
    # image_width
    imageWidthChanged = Signal(int)
    @Property(int, notify=imageWidthChanged)
    def image_width(self):
        if self._image:
            return self._image.width()
        return 1

    # image_height
    imageHeightChanged = Signal(int)
    @Property(int, notify=imageHeightChanged)
    def image_height(self):
        if self._image:
            return self._image.height()
        return 1
    
    def paint(self, painter):
        if self._image:
            pixels = []
            for y in range(self._image.height()):
                for x in range(self._image.width()):
                    pixels.append(self._image.pixel(x, y))
            pixels = [((p / float(pow(2, 32))) - 0.5)*2.0 for p in pixels]
            painter.drawImage(QPoint(0, 0), self._image)