import QtQuick 6.3
import QtTest 1.0
import ShoopDaLoop.PythonBackend

import '../../generated/types.js' as Types
import './testfilename.js' as TestFilename

PythonBackend {
    id: backend
    update_interval_ms: 30
    client_name_hint: 'shoop'
    backend_type: Types.BackendType.JackTest
    backend_argstring: ''

    ShoopTestCase {
        name: 'JackBackend'
        filename : TestFilename.test_filename()

        function test_backend() {
            run_case('test_backend', () => {
                verify(backend.initialized)
                wait(1000)
                if(backend.actual_backend_type != Types.BackendType.JackTest) {
                    skip("Was not able to start a Jack test backend")
                }
            })
        }
    }
}