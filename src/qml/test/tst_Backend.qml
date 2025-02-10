import QtQuick 6.6
import QtTest 1.0

import '..'
import ShoopConstants
import './testfilename.js' as TestFilename

ShoopTestFile {
    Backend {
        id: backend
        update_interval_ms: 30
        client_name_hint: 'ShoopDaLoop'
        backend_type: ShoopConstants.AudioDriverType.Dummy
        driver_setting_overrides: ({})

        ShoopTestCase {
            name: 'DummyBackend'
            filename : TestFilename.test_filename()

            test_fns: ({
                'test_backend': () => {
                    verify(backend.ready)
                    backend.close()
                }
            })
        }
    }
}