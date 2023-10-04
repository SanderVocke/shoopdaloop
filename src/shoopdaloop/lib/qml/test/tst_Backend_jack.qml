import QtQuick 6.3
import QtTest 1.0
import ShoopDaLoop.PythonBackend
import ShoopDaLoop.PythonDummyJackTestServer

import '../../generated/types.js' as Types
import './testfilename.js' as TestFilename

PythonDummyJackTestServer {
    id: server

    PythonBackend {
        id: backend
        update_interval_ms: 30
        client_name_hint: 'ShoopDaLoop'
        backend_type: Types.BackendType.Jack
        backend_argstring: server.get_server_name()

        ShoopTestCase {
            name: 'TwoLoops'
            filename : TestFilename.test_filename()

            function test_backend() {
                run_case('test_backend', () => {
                    verify(backend.initialized)
                    wait(1000)
                    verify_eq(backend.actual_backend_type, Types.BackendType.Jack)
                    backend.close()
                })
            }
        }
    }
}