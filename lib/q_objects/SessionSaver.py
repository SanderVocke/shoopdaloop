from PyQt6.QtCore import QObject, pyqtSlot, pyqtSignal, QThread

import os
import shutil
import tempfile
import tarfile
import time
from threading import Thread
import soundfile as sf
import numpy as np
import resampy

# Asynchronously save session to disk.
class SessionSaver(QThread):
    def __init__(self, parent=None):
        super(SessionSaver, self).__init__(parent)
        
