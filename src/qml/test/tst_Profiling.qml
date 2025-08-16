import QtQuick 6.6

import '..'
import ShoopConstants
import './testfilename.js' as TestFilename

ShoopTestFile {
    Backend {
        id: backend
        update_interval_ms: 10
        client_name_hint: 'ShoopDaLoop'
        backend_type: ShoopConstants.AudioDriverType.Dummy
        driver_setting_overrides: ({})

        ShoopTestCase {
            name: 'Profiling'
            filename : TestFilename.test_filename()
            id: testcase

            test_fns: ({
                'test_backend': () => {
                    testcase.wait(500)
                    testcase.wait_updated(backend)
                    verify(backend.ready)
                    
                    let profiling_data = backend.get_profiling_report();
                    verify(profiling_data !== undefined && profiling_data !== null, "profiling data is undefined");
                }
            })
        }
    }
}