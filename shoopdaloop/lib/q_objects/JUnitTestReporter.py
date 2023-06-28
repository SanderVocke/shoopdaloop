from PySide6.QtCore import QObject, Signal, Property, Slot
from enum import Enum

class Result(Enum):
    Pass = 1,
    Fail = 2,
    Skip = 3,
    Unknown = 4

# Simple object that keeps track of an asynchronous task's execution.
class JUnitTestReporter(QObject):
    def __init__(self, parent=None):
        super(JUnitTestReporter, self).__init__(parent)
        self._results = {
            'n_files': 0,
            'n_cases': 0,
            'n_funcs': 0,
            'n_funcs_passed': 0,
            'n_funcs_failed': 0,
            'n_funcs_skipped': 0,
            'files': {}
        }
    
    # Result is 'pass'/'fail'/'skip'
    @Slot(str, str, str, str)
    def report_result(self, filename, casename, funcname, result):
        if not filename in self._results['files']:
            self._results['files'][filename] = {
                'n_cases': 0,
                'n_funcs': 0,
                'n_funcs_passed': 0,
                'n_funcs_failed': 0,
                'n_funcs_skipped': 0,
                'cases': {}
            }
            self._results['n_files'] += 1
        if not casename in self._results['files'][filename]['cases']:
            self._results['files'][filename]['cases'][casename] = {
                'n_funcs': 0,
                'n_funcs_passed': 0,
                'n_funcs_failed': 0,
                'n_funcs_skipped': 0,
                'funcs': {}
            }
            self._results['files'][filename]['n_cases'] += 1
            self._results['n_cases'] += 1
        if not funcname in self._results['files'][filename]['cases'][casename]['funcs']:
            self._results['files'][filename]['cases'][casename]['funcs'][funcname] = []
            self._results['files'][filename]['cases'][casename]['n_funcs'] += 1
            self._results['files'][filename]['n_funcs'] += 1
            self._results['n_funcs'] += 1
        
        func_result_obj = (Result.Pass if result == 'pass' else (
                           Result.Fail if result == 'fail' else (
                           Result.Skip if result == 'skip' else Result.Unknown
                           )))
        self._results['files'][filename]['cases'][casename]['funcs'][funcname].append( func_result_obj )

        if (func_result_obj == Result.Pass):
            self._results['files'][filename]['cases'][casename]['n_funcs_passed'] += 1
            self._results['files'][filename]['n_funcs_passed'] += 1
            self._results['n_funcs_passed'] += 1
        elif (func_result_obj == Result.Skip):
            self._results['files'][filename]['cases'][casename]['n_funcs_skipped'] += 1
            self._results['files'][filename]['n_funcs_skipped'] += 1
            self._results['n_funcs_skipped'] += 1
        else:
            self._results['files'][filename]['cases'][casename]['n_funcs_failed'] += 1
            self._results['files'][filename]['n_funcs_failed'] += 1
            self._results['n_funcs_failed'] += 1
        
    @Slot(result=str)
    def generate_junit(self):
        r = '<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n'
        r += '<testsuites name="all" tests="{}" failures="{}" skipped="{}">\n'.format(
            self._results['n_funcs'],
            self._results['n_funcs_failed'],
            self._results['n_funcs_skipped']
        )
        for filename,fi in self._results['files'].items():
            for casename,c in fi['cases'].items():
                suitename = '{}::{}'.format(filename, casename)
                r += '  <testsuite name="{}" tests="{}" failures="{}" skipped="{}">\n'.format(
                    suitename,
                    c['n_funcs'],
                    c['n_funcs_failed'],
                    c['n_funcs_skipped']
                )
                for funcname,fu in c['funcs'].items():
                    r += '    <testcase name="{}" tests="{}" failures="{}" skipped="{}"/>\n'.format(
                        funcname,
                        len(fu),
                        fu.count(Result.Fail),
                        fu.count(Result.Skip)
                    )
                r += '  </testsuite>\n'

        r += '</testsuites>\n'
        return r

