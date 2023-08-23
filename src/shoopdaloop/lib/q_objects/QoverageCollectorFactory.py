from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
import json

from ..logging import Logger

class QoverageFileCollector(QObject):
    def __init__(self, filename, inital_lines_data, parent=None):
        super(QoverageFileCollector, self).__init__(parent)
        self.logger = Logger("Qoverage")
        self.filename = filename
        self.lines_data = inital_lines_data
    
    @Slot(list)
    def trace(self, lines):
        for line in lines:
            if self.lines_data[line] != None:
                self.lines_data[line] += 1
    
    @Slot()
    def report(self):
        self.logger.info(
            '<QOVERAGERESULT file={}>{}</QOVERAGERESULT>'.format(
                self.filename,
                json.dumps(self.lines_data)
            )
        )

class QoverageCollectorFactory(QObject):
    def __init__(self, parent=None):
        super(QoverageCollectorFactory, self).__init__(parent)
        self.logger = Logger("QoverageCollectorFactory")
        self.file_collectors = []
    
    @Slot(str, list, result='QVariant')
    def create_file_collector(self, filename, initial_lines_data):
        self.logger.debug("Collector requested for {}".format(filename))
        rval = QoverageFileCollector(filename, initial_lines_data, self)
        self.file_collectors.append(rval)
        return rval
    
    def report_all(self):
        self.logger.debug("Reporting all")
        if len(self.file_collectors) > 0:
            self.logger.info('Reporting {} file collectors'.format(len(self.file_collectors)))
            for fc in self.file_collectors:
                fc.report()
