import QtQuick 6.3
import QtTest 1.0
import ShoopDaLoop.PythonBackend

import '../../generated/types.js' as Types
import './testfilename.js' as TestFilename

PythonBackend {
    id: backend
    update_interval_ms: 30
    client_name_hint: 'ShoopDaLoop'
    backend_type: Types.AudioDriverType.Dummy

    ShoopTestCase {
        name: 'DummyBackend'
        filename : TestFilename.test_filename()

        test_fns: ({
            'test_backend': () => {
                verify(backend.initialized)
                backend.close()
            }
        })
    }
}