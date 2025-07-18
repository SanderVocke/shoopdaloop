import QtQuick 6.6
import QtTest 1.0

import ShoopConstants
import './testfilename.js' as TestFilename
import '..'

ShoopTestFile {
    Backend {
        id: backend
        update_interval_ms: 30
        client_name_hint: 'shoop'
        backend_type: backend_type_is_supported(ShoopConstants.AudioDriverType.JackTest) ?
                      ShoopConstants.AudioDriverType.JackTest : ShoopConstants.AudioDriverType.Dummy
        driver_setting_overrides: ({})

        ShoopTestCase {
            name: 'JackBackend'
            filename : TestFilename.test_filename()
            when: {
                let ready = backend.ready || backend.backend_type == null
                return ready
            }

            test_fns: ({
                'test_backend_jack': () => {
                    if(!backend.backend_type_is_supported(ShoopConstants.AudioDriverType.JackTest)) {
                        skip("Backend was built without Jack support")
                        backend.close()
                        return
                    }

                    verify(backend.ready)
                    wait(1000)
                    verify_eq(
                        backend.actual_backend_type,
                        ShoopConstants.AudioDriverType.JackTest,
                        "Was not able to start a Jack test backend even though support should be available"
                    )

                    backend.close()
                }
            })
        }
    }
}