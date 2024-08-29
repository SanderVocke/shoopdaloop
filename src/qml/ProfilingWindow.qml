import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6
import ShoopDaLoop.PythonLogger

// The click track dialog allows the user to interactively configure, preview
// and select a generated click track clip.
ApplicationWindow {
    id: root
    title: "Profiling"

    width: 400
    height: 450

    Material.theme: Material.Dark

    property int cycle_us : 0
    property var backend : null
    property PythonLogger logger : PythonLogger { name: 'Frontend.ProfilingDialog' }
    property var profiling_data : null
    property var profiling_tree_model : {
        if (profiling_data) {
            // Round any floating-point values to integers
            // and provide data as percent of process cycle
            var _data = {}

            var bufsize = root.backend.get_buffer_size()
            var samplerate = root.backend && root.backend.initialized ? root.backend.get_sample_rate() : 1
            cycle_us = bufsize / samplerate * 1000000.0

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

    property alias auto_update: auto_update_switch.checked

    function update() {
        var data = root.backend.get_profiling_report()
        var bufsize = root.backend.get_buffer_size()
        var samplerate = root.backend && root.backend.initialized ? root.backend.get_sample_rate() : 1
        cycle_us = bufsize / samplerate * 1000000.0

        root.profiling_data = data
    }

    Timer {
        id: update_timer
        interval: 1000
        repeat: true
        running: root.visible && root.auto_update
        onTriggered: root.update()
    }

    Row {
        id: controls_row

        anchors {
            left: parent.left
            top: parent.top
            right: parent.right
        }

        Button {
            text: "Update"
            id: btn

            onClicked: root.update()
        }

        Switch {
            id: auto_update_switch
            text: "Auto-update"
            anchors.verticalCenter : btn.verticalCenter
        }
    }

    Label {
        id: cycle_time_label
        anchors.top: controls_row.bottom
        text: "System cycle time (us): " + root.cycle_us
    }

    TreeView {
        id: tree
       
        rowHeightProvider: (idx) => 20

        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
            top: cycle_time_label.bottom
        }

        delegate: TreeViewDelegate {
            id: delegate
            background: Rectangle { color: 'transparent' }
        }

        model: root.profiling_tree_model
        onModelChanged: expander.trigger()

        ExecuteNextCycle {
            id: expander
            onExecute: if(tree.model) { tree.expandRecursively() }
        }
    }
}
