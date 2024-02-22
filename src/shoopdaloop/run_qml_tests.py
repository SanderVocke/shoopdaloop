#!/usr/bin/python
import shoopdaloop.qml_tests

def run_qml_tests():
    import sys
    exit(shoopdaloop.qml_tests.run_qml_tests(sys.argv[1:]))