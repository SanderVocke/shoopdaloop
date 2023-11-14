from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQml import QJSValue
from PySide6.QtTest import QTest

from .TestCase import TestCase

from ..logging import Logger

qtest = QTest()

class TestRunner(QObject):
    def __init__(self, parent=None):
        super(TestRunner, self).__init__(parent)
        self.registered_testcases = []
        self.logger = Logger('Frontend.TestRunner')
        self.running = None
        self.ran = []
        
    @Slot(TestCase)
    def register_testcase(self, testcase):
        self.logger.info('Registering testcase {}'.format(testcase))
        self.registered_testcases.append(testcase)
        self.update()
        
    @Slot()
    def update(self):
        if self.running == None and len(self.registered_testcases) > 0:
            self.run(self.registered_testcases.pop(0))
    
    @Slot(TestCase)
    def run(self, testcase):
        self.logger.info('---- testcase {} ----'.format(testcase.name))
        self.running = testcase
        self.running.run()
        self.running = None
        self.ran.append(testcase)
    
    @Slot(int)
    def wait(self, ms):
        qtest.qWait(ms)

