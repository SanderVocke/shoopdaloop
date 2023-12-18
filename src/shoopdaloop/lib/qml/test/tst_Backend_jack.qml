import QtQuick 6.3
import QtTest 1.0
import ShoopDaLoop.PythonBackend

import ShoopConstants
import './testfilename.js' as TestFilename

PythonBackend {
    id: backend
    update_interval_ms: 30
    client_name_hint: 'shoop'
    backend_type: ShoopConstants.AudioDriverType.JackTest

    ShoopTestCase {
        name: 'JackBackend'
        filename : TestFilename.test_filename()

        test_fns: ({
            'test_backend_jack': () => {
                if(!backend.backend_type_is_supported(ShoopConstants.AudioDriverType.JackTest)) {
                    skip("Backend was built without Jack support")
                    return
                }

                verify(backend.initialized)
                wait(1000)
                verify_eq(
                    backend.actual_backend_type,
                    ShoopConstants.AudioDriverType.JackTest,
                    "Was not able to start a Jack test backend even though support should be available"
                )
            }
        })
    }
}