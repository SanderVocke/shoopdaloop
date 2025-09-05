import QtQuick 6.6
import ShoopDaLoop.Rust
import './testDeepEqual.js' as TestDeepEqual
import '../js/generate_session.js' as GenerateSession
import './testfilename.js' as TestFilename
import '..'

ShoopTestFile {
    TestSession {
        id: session

        anchors.fill: parent
        initial_descriptor: {
            let _session = GenerateSession.generate_default_session(global_args.version_string, null, true, 1, 1, [])
            return _session
        }

        ShoopSessionTestCase {
            id: testcase
            name: 'FetchDisplayData'
            filename : TestFilename.test_filename()
            session: session

            RegistryLookup {
                registry: registries.state_registry
                key: 'main_details_pane'
                id: lookup_main_details_pane
            }
            
            function details_pane() {
                return lookup_main_details_pane.object
            }

            function sync_loop() {
                return session.sync_track.loops[0]
            }

            function sync_loop_channel() {
                if (!sync_loop()) return []
                var r = sync_loop().get_audio_output_channels()
                return r[0]
            }

            function clear() {
                sync_loop().clear()
                testcase.wait_updated(session.backend)
                verify_loop_cleared(sync_loop())
                registries.state_registry.reset()
            }

            test_fns: ({
                'test_load_and_display': () => {
                    check_backend()
                    clear()

                    registries.state_registry.add_to_set('selected_loop_ids', sync_loop().obj_id)

                    sync_loop_channel().load_data(new Array(50000).fill(0.1))

                    testcase.wait_updated(session.backend)

                    registries.state_registry.set_details_open(true)
                    testcase.wait_updated(session.backend)
                    testcase.wait(100)

                    registries.state_registry.set_details_open(false)
                    testcase.wait_updated(session.backend)
                    testcase.wait(100)

                    registries.state_registry.set_details_open(true)
                    testcase.wait_updated(session.backend)
                    testcase.wait(100)

                    registries.state_registry.set_details_open(false)
                    testcase.wait_updated(session.backend)
                    testcase.wait(100)

                    registries.state_registry.set_details_open(true)
                    testcase.wait_updated(session.backend)
                    testcase.wait(100)
                },
            })
        }
    }
}