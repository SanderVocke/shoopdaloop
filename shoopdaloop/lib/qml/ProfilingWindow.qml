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
    property var profiling_data : null
    property var profiling_tree_model : {
        if (profiling_data) {
            // Round any floating-point values to integers
            // and provide data as percent of process cycle
            var _data = {}

            var bufsize = root.backend.get_buffer_size()
            var samplerate = root.backend.get_sample_rate()
            var cycle_us = bufsize / samplerate * 1000000.0

            Object.entries(profiling_data).forEach((p) => {
                _data[p[0]] = {}
                Object.entries(p[1]).forEach((pp) => {
                    var asfloat = parseFloat(pp[1])
                    if (asfloat != NaN) {
                        _data[p[0]][pp[0]] = parseInt(asfloat)
                        var percentkey = pp[0] + '_%'
                        _data[p[0]][percentkey] = parseInt(asfloat / cycle_us * 100.0)
                    } else {
                        // Not a number. no rounding or percentages
                        _data[p[0]][pp[0]] = pp[1]
                    }

                    _data[p[0]][pp[0]] =
                        parseFloat(pp[1]) != NaN ?
                        parseInt(pp[1]) : pp[1]
                })
            })
            return tree_model_factory.create(_data, '.', column_roles, column_headers)
        }
        return null;
    }
    readonly property var column_roles : ['name', 'average', 'worst', 'average_%', 'worst_%']
    readonly property var column_headers : ['Section', 'Avg', 'Worst', 'Avg (%)', 'Worst (%)']

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

            root.profiling_data = data
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
            id: delegate
            background: Rectangle { color: 'transparent' }
        }

        model: root.profiling_tree_model
    }
}
