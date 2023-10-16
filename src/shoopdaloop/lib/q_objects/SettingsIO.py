from PySide6.QtCore import QObject, Signal, Property, Slot, QSettings

import appdirs

class SettingsIO(QObject):
    def __init__(self, parent=None):
        super(SettingsIO, self).__init__(parent)
        self.filename = appdirs.user_config_dir(appname='ShoopDaLoop') + '/settings.json'
        
    @Slot(str, 'QVariant')
    def save_settings(self, settings, override_filename=None):
        file = self.filename if override_filename is None else override_filename
        with open(file, 'w') as f:
            f.write(settings)
    
    @Slot(str, result='QVariant')
    def load_settings(self, override_filename=None):
        file = self.filename if override_filename is None else override_filename
        with open(file, 'r') as f:
            return f.read()