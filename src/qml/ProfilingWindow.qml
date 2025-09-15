import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6
import Qt.labs.qmlmodels

// The click track dialog allows the user to interactively configure, preview
// and select a generated click track clip.
ShoopApplicationWindow {
    id: root
    title: "Profiling"

    width: 400
    height: 450

    property int cycle_us : 0
    property var backend : null
    property ShoopRustLogger logger : ShoopRustLogger { name: 'Frontend.ProfilingDialog' }
    property var profiling_data : null

    property var profiling_data_rows: {
        if (!profiling_data) { return [] }
        return Object.keys(profiling_data).map((key) =>
            ({
                item: key,
                average: Math.round(profiling_data[key].average),
                worst: Math.round(profiling_data[key].worst)
            })
        )
    }

    TableModel {
        id: table_model
        TableModelColumn {
            display: "item"
        }
        TableModelColumn {
            display: "average"
        }
        TableModelColumn {
            display: "worst"
        }

        property var header_row: ({ item: "Item", average: "Avg", worst: "Worst" })

        rows: profiling_data_rows ? [
            header_row,
            ...profiling_data_rows
        ] : [header_row]
    }

    property alias auto_update: auto_update_switch.checked

    function update() {
        var data = root.backend.get_profiling_report()

        if (data === undefined || data === null) {
            root.logger.error("Could not get profiling report to display.");
            return
        }

        var bufsize = root.backend.get_buffer_size()
        var samplerate = root.backend && root.backend.ready ? root.backend.sample_rate : 1
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

    TableView {
        id: table
        model: table_model

        rowHeightProvider: (idx) => 20

        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
            top: cycle_time_label.bottom
        }

        delegate: DelegateChooser {
            DelegateChoice {
                row: 0
                Label {
                    padding: 10
                    font.bold: true
                    text: model.display
                }
            }
            DelegateChoice {
                Label {
                    padding: 10
                    text: model.display
                }
            }
        }
    }
}
