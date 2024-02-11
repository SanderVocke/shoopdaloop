from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtTest import QTest
from .ShoopPyObject import *

from .TestCase import TestCase

from ..logging import Logger

class TestRunner(ShoopQObject):
    def __init__(self, parent=None):
        super(TestRunner, self).__init__(parent)
        self.registered_testcases = set()
        self.logger = Logger('Frontend.TestRunner')
        self.running = None
        self.ran = set()
        self._results = dict()
        self.skip_fn = lambda fn: False
        
    def is_done(self):
        return len(self.registered_testcases) > 0 and len(self.ran) == len(self.registered_testcases)
    
    # done
    doneChanged = Signal()
    @Property(bool, notify=doneChanged)
    def done(self):
        return self.is_done()
    
    # results
    @Property('QVariant')
    def results(self):
        return self._results
        
    @Slot(TestCase)
    def register_testcase(self, testcase):
        self.logger.info('Registering testcase {}'.format(testcase))
        self.registered_testcases.add(testcase)
    
    @Slot(TestCase)
    def testcase_ready_to_start(self, testcase):
        self.logger.info('Testcase {} ready to start'.format(testcase))
        self.run(testcase)
    
    @Slot(TestCase, str, str)
    def testcase_ran_fn(self, testcase, fn_name, status):
        name = testcase.name        
        if not name in self._results:
            self._results[name] = dict()
        self._results[testcase.name][fn_name] = status
    
    @Slot(TestCase, str)
    def testcase_register_fn(self, testcase, fn_name):
        name = testcase.name        
        if not name in self._results:
            self._results[name] = dict()
        self._results[testcase.name][fn_name] = 'fail'

    @Slot(str, result=bool)
    def should_skip(self, fn):
        return self.skip_fn(fn)

    @Slot(TestCase)
    def run(self, testcase):
        self.running = testcase
        self.logger.info('---- testcase {} ----'.format(testcase.name))
        self.running.run()
        self.running = None

        self.ran.add(testcase)

