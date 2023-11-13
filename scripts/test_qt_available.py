#!/usr/bin/env python3
from shoopdaloop.lib.ensure_ld_qt import ensure_ld_qt

result = 0

try:
    ensure_ld_qt()
    print("All required Qt libraries are available.")
except Exception as e:
    print("ShoopDaLoop backend library is not available: {}".format(e))
    result = 1

try:
    from shoopdaloop.libshoopdaloop_bindings import *
    print("ShoopDaLoop backend library is available.")
except Exception as e:
    print("ShoopDaLoop backend library is not available: {}".format(e))
    result = 1

try:
    from shoopdaloop.qml_tests import *
    print("ShoopDaLoop unit test runner is available.")
except Exception as e:
    print("ShoopDaLoop unit test runner is not available: {}".format(e))
    result = 1

try:
    from shoopdaloop.qt_extensions import *
    print("ShoopDaLoop Qt extensions are available.")
except Exception as e:
    print("ShoopDaLoop Qt extensions are not available: {}".format(e))
    result = 1

exit(result)
