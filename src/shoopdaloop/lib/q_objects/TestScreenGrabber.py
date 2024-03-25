from PySide6.QtCore import QObject, Signal, Property, Slot, QSettings
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
    
    @ShoopSlot(str)
    def grab_all(self, output_folder):
        os.makedirs(output_folder, exist_ok=True)

        maybe_engine = self.weak_engine() if self.weak_engine else None
        if maybe_engine:
            roots = maybe_engine.rootObjects()
            windows = set()
            for root in roots:
                if (isinstance(root, QQuickWindow)):
                    windows.add(root)
                windows.update(root.findChildren(QQuickWindow))
        
        self.logger.debug(lambda: 'Found {} windows.'.format(len(windows)))

        images = dict()
        for window in windows:
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
