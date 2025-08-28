from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from .ShoopPyObject import *

from .QoverageCollectorFactoryImpl import *
class QoverageFileCollector(ShoopQObject):
    def __init__(self, filename, inital_lines_data, impl, parent=None):
        super(QoverageFileCollector, self).__init__(parent)
        self.impl = impl
    
    @ShoopSlot(list)
    def trace(self, lines):
        self.impl.trace(lines)

    @ShoopSlot()
    def on_about_to_quit(self):
        self.impl.on_about_to_quit()
    
    def report(self):
        self.impl.report()

class QoverageCollectorFactory(ShoopQObject):
    def __init__(self, parent=None):
        super(QoverageCollectorFactory, self).__init__(parent)
        self.impl = QoverageCollectorFactoryImpl()
    
    @ShoopSlot(str, list, result='QVariant')
    def create_file_collector(self, filename, initial_lines_data):
        impl = self.impl.create_file_collector()
        return QoverageFileCollector(filename, initial_lines_data, impl, self)
    
    def report_all(self):
        self.impl.report_all()
