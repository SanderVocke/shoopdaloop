import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6
// Pinned: the 2D GraphsView API used below (axisX/axisY on the view,
// invokable append()/clear() on the series) only exists from Qt 6.8 on.
// Pinning the version turns an older Qt into a hard import error instead
// of a confusing "cannot assign to non-existent property" at runtime.
import QtGraphs 6.8

Item {
    id: root

    property int max_points: 1000
    function add_point(buffers_created, buffers_available) {
        root.points.push(buffers_available)
        root.points.splice(0, Math.max(0,root.points.length - max_points))
        root.points_changed = true
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
            if (root.points_changed && root.visible) {
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
            id: y_axis
            max: 1
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
        y_axis.max = root.points.length > 0 ?
                    Math.max(...root.points) * 1.1 :
                    1
        root.points_changed = false
    }
}
