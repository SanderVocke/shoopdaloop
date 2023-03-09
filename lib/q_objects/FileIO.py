from PyQt6.QtCore import QObject, pyqtSlot

import os
import shutil
import tempfile
import tarfile

# Allow text file I/O from QML
class FileIO(QObject):
    def __init__(self, parent=None):
        super(FileIO, self).__init__(parent)
    
    ######################
    # SLOTS
    ######################

    @pyqtSlot(str, str)
    def write_file(self, filename, content):
        with open(filename, 'w') as file:
            file.write(content)

    @pyqtSlot(str, result=str)
    def read_file(self, filename):
        r = None
        with open(filename, 'r') as file:
            r = file.read()
        return r
    
    @pyqtSlot(result=str)
    def create_temporary_folder(self):
        return tempfile.mkdtemp()
    
    @pyqtSlot(result=str)
    def create_temporary_file(self):
        return tempfile.mkstemp()[1]

    @pyqtSlot(str)
    def delete_recursive(self, folder):
        shutil.rmtree(folder)

    @pyqtSlot(str, str, bool)
    def make_tarfile(self, filename, source_dir, compress):
        flags = ("w:gz" if compress else "w")
        with tarfile.open(filename, flags) as tar:
            tar.add(source_dir, arcname=source_dir)