import QtQuick 6.6

import ShoopDaLoop.Rust
import '../js/generate_session.js' as GenerateSession
import './testfilename.js' as TestFilename
import '..'

ShoopTestFile {
    id: root

    Backend {
        id: backend
        backend_type: ShoopRustConstants.AudioDriverType.Dummy
        update_interval_ms: 10
    }

    property var track_descriptor: GenerateSession.generate_default_track(
        "descriptor16",
        1,
        "descriptor16",
        false,
        "descriptor16",
        16,
        16,
        0,
        true,
        false,
        false,
        "carla_patchbay_16"
    )
    property var alias_track_descriptor: GenerateSession.generate_default_track(
        "descriptor16alias",
        1,
        "descriptor16alias",
        false,
        "descriptor16alias",
        16,
        16,
        0,
        true,
        false,
        false,
        "carla_patchbay_16x"
    )

    ShoopTestCase {
        name: 'Drywet_carla_patchbay_16_descriptor'
        filename: TestFilename.test_filename()

        function verify_descriptor(descriptor) {
            let component = Qt.createComponent("../FXChain.qml")
            verify_eq(component.status, Component.Ready, component.errorString())
            let chain = component.createObject(root, {
                "backend": backend,
                "descriptor": descriptor.fx_chain
            })
            verify_true(chain)
            verify_eq(chain.chain_type, ShoopRustConstants.FXChainType.CarlaPatchbay16x)
            chain.destroy()
        }

        test_fns: ({
            // Purpose: The user-facing Patchbay 16 descriptor must instantiate the 16-channel host.
            // Use case: Selecting Carla Patchbay 16x when adding a track creates the requested processor.
            'test_carla_patchbay_16_descriptor_selects_16x_backend': () => {
                verify_descriptor(track_descriptor)
            },

            // Purpose: The backend-style Patchbay 16x alias must instantiate the same host.
            // Use case: A session saved with the alternate spelling remains load-compatible.
            'test_carla_patchbay_16x_descriptor_alias_selects_16x_backend': () => {
                verify_descriptor(alias_track_descriptor)
            }
        })
    }
}
