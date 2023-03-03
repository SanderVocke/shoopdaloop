import QtQuick 2.3
import QtTest 1.0
import Backend

import '../../../build/types.js' as Types
import '../../generate_session.js' as GenerateSession
import '..'

Backend {
    id: backend
    update_interval_ms: 30
    client_name_hint: 'ShoopDaLoop'
    backend_type: Types.BackendType.Dummy

    Session {
        id: session
        anchors.fill: parent
        initial_descriptor: GenerateSession.generate_default_session()

        TestCase {
            Timer {
                id: timer
                running: session.loaded
                interval: 10
                repeat: false
                property bool done : false
                onTriggered: done = true
            }

            when: timer.done

            function test_session_descriptor_default() {
                verify(backend.initialized, "backend not initialized")
                compare(session.tracks.length, 2)
                var master_loops = session.tracks[0].all_loops()
                compare(master_loops.length, 1)
                console.log("HELLOWORLD", session.state_registry)
                compare(master_loops[0].is_master, true)
            }

            function cleanupTestCase() { backend.close() }
        }
    }
}