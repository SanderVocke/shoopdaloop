import QtQuick 6.6
import ShoopDaLoop.Rust
import '..'
import './testfilename.js' as TestFilename

ShoopTestFile {
    Backend {
        id: backend
        update_interval_ms: 10
        client_name_hint: 'ShoopDaLoop'
        backend_type: ShoopRustConstants.AudioDriverType.Dummy
        driver_setting_overrides: ({})

        ShoopTestCase {
            name: 'DummyBackend'
            filename : TestFilename.test_filename()

            test_fns: ({
                'test_explicit_refresh_epoch': () => {
                    verify(backend.ready)
                    let before = backend.refresh_epoch
                    backend.refresh()
                    verify_true(backend.refresh_epoch > before)
                },
                'test_fallback_refresh': () => {
                    verify(backend.ready)
                    let before = backend.refresh_epoch
                    wait_condition(() => backend.refresh_epoch > before, 1000,
                                   "GUI fallback refresh did not publish")
                },
                'test_backend': () => {
                    verify(backend.ready)
                    backend.close()
                }
            })
        }
    }
}