from .TestCase import TestCase

from ..logging import Logger

class TestRunnerImpl():
    def __init__(self):
        self.registered_testcases = set()
        self.logger = Logger('Frontend.TestRunner')
        self.running_testcase = None
        self.ran_testcases = set()
        self.testcase_results = dict()
        # self.skip_fn = lambda fn: False
        
    def is_done(self):
        return len(self.registered_testcases) > 0 and len(self.ran_testcases) == len(self.registered_testcases)
    
    # done
    def get_done(self):
        return self.is_done()
    
    # results
    def results(self):
        return self.testcase_results
        
    ###########
    ## SLOTS
    ###########

    def register_testcase(self, testcase):
        self.logger.info('Registering testcase {}'.format(testcase))
        self.registered_testcases.add(testcase)
    
    def testcase_ready_to_start(self, testcase):
        self.logger.info('Testcase {} ready to start'.format(testcase))
        self.run(testcase)
    
    def testcase_ran_fn(self, testcase, fn_name, status):
        name = testcase.name        
        if not name in self.testcase_results:
            self.testcase_results[name] = dict()
        self.testcase_results[testcase.name][fn_name] = status
    
    def testcase_register_fn(self, testcase, fn_name):
        name = testcase.name        
        if not name in self.testcase_results:
            self.testcase_results[name] = dict()
        self.testcase_results[testcase.name][fn_name] = 'fail'

    def should_skip(self, fn):
        return self.skip_fn(fn)

    def run(self, testcase):
        self.running_testcase = testcase
        self.logger.info('-------------------------------------------------')
        self.logger.info('-- testcase {}'.format(testcase.name))
        self.logger.info('-------------------------------------------------')
        self.running_testcase.run()
        self.running_testcase = None

        self.ran_testcases.add(testcase)

