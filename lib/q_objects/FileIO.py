from PyQt6.QtCore import QObject, pyqtSlot

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