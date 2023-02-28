import os
import sys
sys.path.append(
    os.path.dirname(os.path.realpath(__file__)) + '/../build/backend/frontend_interface'
)

import lib.backend_config as backend_config

if backend_config.test_backend:
    from shoopdaloop_qt_test_backend import *
else:
    from shoopdaloop_backend import *