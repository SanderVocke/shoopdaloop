from .JUnitTestReporter import JUnitTestReporter

def test_basic():
    reporter = JUnitTestReporter()
    reporter.report_result('myfile', 'mycase', 'myfunc', 'pass')
    reporter.report_result('myfile', 'mycase', 'myfunc2', 'fail')
    reporter.report_result('myfile2', 'mycase2', 'myfunc3', 'skip')

    assert reporter.generate_junit() == \
'''<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="all" tests="3" failures="1" skipped="1">
  <testsuite name="myfile::mycase" tests="2" failures="1" skipped="0">
    <testcase name="myfunc" tests="1" failures="0" skipped="0"/>
    <testcase name="myfunc2" tests="1" failures="1" skipped="0"/>
  </testsuite>
  <testsuite name="myfile2::mycase2" tests="1" failures="0" skipped="1">
    <testcase name="myfunc3" tests="1" failures="0" skipped="1"/>
  </testsuite>
</testsuites>
'''