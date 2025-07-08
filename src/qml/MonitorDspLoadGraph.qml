import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6
import QtGraphs
import ShoopDaLoop.PythonLogger

Item {
    id: root

    property int max_points: 1000
    function add_point(load) {
        let cpy = root.points.slice(Math.max(0,root.points.length - max_points - 1))
        cpy.push(load)
        root.points = cpy
    }

    property var points : []
    property bool points_changed : true

    Component.onCompleted: {
        updateChart()
    }
    onPointsChanged: {
        root.points_changed = true
    }

    Timer {
        interval: 500
        running: true
        repeat: true
        onTriggered: {
            if (root.points_changed) {
                root.updateChart()
            }
        }
    }

    GraphsView {
        anchors.fill: parent

        axisX: ValueAxis {
            min: 0
            max: root.max_points
        }
        axisY: ValueAxis {
            max: root.points.length > 0 ?
                    Math.ceil((Math.max(...root.points) * 1.1) / 5.0) * 5.0 :
                    100.0
            min: 0
        } 
        LineSeries {
            id: line_series
        }
    }

    function updateChart() {
        line_series.clear()
        for (var i = 0; i < root.points.length; i++) {
            line_series.append(i, root.points[i])
        }
        root.points_changed = false
    }
}
