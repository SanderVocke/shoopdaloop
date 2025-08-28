import appdirs
import json
import os

from ..logging import *

class SettingsIOImpl():
    def __init__(self):
        self.dir = appdirs.user_config_dir(appname='ShoopDaLoop')
        self.filename = self.dir + '/settings.json'
        self.logger = Logger('Frontend.SettingsIO')
        
    def save_settings(self, settings, override_filename=None):
        file = self.filename if override_filename is None else override_filename
        self.logger.info(lambda: "Saving settings to {}".format(file))
        if not os.path.exists(self.dir):
            os.makedirs(self.dir)
        with open(file, 'w') as f:
            f.write(json.dumps(settings, indent=2))
    
    def load_settings(self, override_filename=None):
        file = self.filename if override_filename is None else override_filename
        if os.path.exists(file):
            self.logger.info(lambda: "Loading settings from {}".format(file))
            with open(file, 'r') as f:
                return json.loads(f.read())
        else:
            self.logger.info(lambda: "No settings file found at {}. Using defaults.".format(file))
            return None