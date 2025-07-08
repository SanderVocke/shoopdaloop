import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6
import QtGraphs
import ShoopDaLoop.PythonLogger

ApplicationWindow {
    id: root
    title: "Monitor"

    width: 600
    height: 450

    Material.theme: Material.Dark

    function add_dsp_load_point(load) {
        dsp_load.add_point(load)
    }

    ShoopComboBox {
        id: combo
        anchors.top: parent.top
        anchors.left: parent.left
        width: 200

        model: ["DSP load"]
    }

    StackLayout {
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
            top: combo.bottom
        }
        currentIndex: combo.currentIndex

        MonitorDspLoadGraph {
            id: dsp_load
        }
    }
}
