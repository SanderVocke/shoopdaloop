import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3
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
    property alias profiling_tree_model : tree.model
    readonly property var column_roles : ['name', 'average', 'worst', 'most_recent']

    Button {
        text: "Update"
        id: btn

        anchors {
            left: parent.left
            top: parent.top
            right: parent.right
        }

        onClicked: {
            var data = root.backend.get_profiling_report()
            root.logger.info(JSON.stringify(data))
            var bufsize = root.backend.get_buffer_size()
            var samplerate = root.backend.get_sample_rate()
            var us = bufsize / samplerate * 1000000.0
            root.logger.info(`Backend buf size: ${bufsize} (${us} us)`)

            root.profiling_tree_model = tree_model_factory.create(data, '.', root.column_roles)
        }
    }

    TreeView {
        id: tree

        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
            top: btn.bottom
        }
        delegate: TreeViewDelegate {
            background: Rectangle { color: 'black' }
        }

        onModelChanged: {
            console.log("model changed!", model)
        }
    }
}
