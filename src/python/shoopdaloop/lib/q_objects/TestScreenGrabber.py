from PySide6.QtCore import QObject, Signal, Property, Slot
from PySide6.QtQml import QJSValue
from PySide6.QtQuick import QQuickWindow
from .ShoopPyObject import *

import weakref
import os

from ..logging import *

class TestScreenGrabber(ShoopQObject):
    def __init__(self, weak_engine=None, parent=None):
        super(TestScreenGrabber, self).__init__(parent)
        self.weak_engine = weak_engine
        self.logger = Logger('Frontend.TestScreenGrabber')
        self.windows = set()

    @ShoopSlot("QVariant")
    def add_window(self, window):
        if not isinstance(window, QQuickWindow):
            self.logger.warning(lambda: f"Got a non-QQuickWindow window: {window}")
            return
        self.logger.debug(lambda: f"Adding window: {window}")
        self.windows.add(window)

    @ShoopSlot("QVariant")
    def remove_window(self, window):
        if window in self.windows:
            self.logger.debug(lambda: f"Removing window: {window}")
            self.windows.remove(window)
    
    @ShoopSlot(str)
    def grab_all(self, output_folder):
        os.makedirs(output_folder, exist_ok=True)
        
        self.logger.info(lambda: 'Got {} windows to capture.'.format(len(self.windows)))

        images = dict()
        for window in self.windows:
            image = window.grabWindow()
            filename = window.title().lower().replace(' ','_')
            if not filename:
                filename = 'unnamed'
            if filename in images.keys():
                i = 1
                while filename + '_' + str(i) in images:
                    i += 1
                filename += '_' + str(i)
            images[filename] = image
        
        for filename,image in images.items():
            self.logger.debug(lambda: 'Saving {}...'.format(filename))
            image.save(output_folder + '/' + filename + '.png')
