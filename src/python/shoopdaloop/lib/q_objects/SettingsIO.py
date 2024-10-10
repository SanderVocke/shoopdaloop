from PySide6.QtCore import QObject, Signal, Property, Slot, QSettings
from PySide6.QtQml import QJSValue
from .ShoopPyObject import *

import appdirs
import json
import os

from ..logging import *

class SettingsIO(ShoopQObject):
    def __init__(self, parent=None):
        super(SettingsIO, self).__init__(parent)
        self.dir = appdirs.user_config_dir(appname='ShoopDaLoop')
        self.filename = self.dir + '/settings.json'
        self.logger = Logger('Frontend.SettingsIO')
        
    @ShoopSlot('QVariant', 'QVariant')
    def save_settings(self, settings, override_filename=None):
        if isinstance(settings, QJSValue):
            settings = settings.toVariant()
        file = self.filename if override_filename is None else override_filename
        self.logger.info(lambda: "Saving settings to {}".format(file))
        if not os.path.exists(self.dir):
            os.makedirs(self.dir)
        with open(file, 'w') as f:
            f.write(json.dumps(settings, indent=2))
    
    @ShoopSlot('QVariant', result='QVariant')
    def load_settings(self, override_filename=None):
        file = self.filename if override_filename is None else override_filename
        if os.path.exists(file):
            self.logger.info(lambda: "Loading settings from {}".format(file))
            with open(file, 'r') as f:
                return json.loads(f.read())
        else:
            self.logger.info(lambda: "No settings file found at {}. Using defaults.".format(file))
            return None