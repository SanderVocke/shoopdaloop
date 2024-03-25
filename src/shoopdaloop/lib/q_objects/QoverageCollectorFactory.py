from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
import json

from .ShoopPyObject import *

from ..logging import Logger

class QoverageFileCollector(ShoopQObject):
    def __init__(self, filename, inital_lines_data, parent=None):
        super(QoverageFileCollector, self).__init__(parent)
        self.filename = filename
        self.lines_data = inital_lines_data
    
    @ShoopSlot(list)
    def trace(self, lines):
        for line in lines:
            if self.lines_data[line] != None:
                self.lines_data[line] += 1

    @ShoopSlot()
    def on_about_to_quit(self):
        # Ignore this signal. It is the method Qoverage usually uses to trigger reporting,
        # but we have our own.
        pass
    
    def report(self):
        print(f'<QOVERAGERESULT file="{self.filename}">{json.dumps(self.lines_data)}</QOVERAGERESULT>')

class QoverageCollectorFactory(ShoopQObject):
    def __init__(self, parent=None):
        super(QoverageCollectorFactory, self).__init__(parent)
        self.logger = Logger("QoverageCollectorFactory")
        self.logger.debug(lambda: "Initialized")
        self.file_collectors = {}
    
    @ShoopSlot(str, list, result='QVariant')
    def create_file_collector(self, filename, initial_lines_data):
        # When running QML unit tests, the same qml files will get re-loaded and the same
        # collectors re-requested. Ensure we pass back the existing collectors such that
        # total coverage is added, not reset from scratch.
        if filename in self.file_collectors:
            self.logger.trace(lambda: "Request existing collector for {}".format(filename))
            return self.file_collectors[filename]
        else:
            self.logger.debug(lambda: "New collector for {}".format(filename))
            rval = QoverageFileCollector(filename, initial_lines_data, self)
            self.file_collectors[filename] = rval
            return rval
    
    def report_all(self):
        self.logger.debug(lambda: "Reporting all")
        if len(self.file_collectors.items()) > 0:
            self.logger.info(lambda: 'Reporting {} file collectors'.format(len(self.file_collectors)))
            for fc in self.file_collectors.values():
                fc.report()
