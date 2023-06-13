import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15
import Logger

// The click track dialog allows the user to interactively configure, preview
// and select a generated click track clip.
ApplicationWindow {
    id: root

    width: 400
    height: 450

    Material.theme: Material.Dark

    property var backend : null
    property Logger logger : Logger { name: 'Frontend.ProfilingDialog' }

    Button {
        text: "Report"

        onClicked: {
            root.logger.info(JSON.stringify(root.backend.get_profiling_report()))
            var bufsize = root.backend.get_buffer_size()
            var samplerate = root.backend.get_sample_rate()
            var us = bufsize / samplerate * 1000000.0
            root.logger.info(`Backend buf size: ${bufsize} (${us} us)`)
        }
    }
}
