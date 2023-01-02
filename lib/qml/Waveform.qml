import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

Item {
    id: chart
    property alias data: cnv.data

    Canvas {
        id: cnv
        property var data
        anchors.fill: parent
        contextType: "2d"
        onPaint: {
            console.log ('pnt'); 
            var ctx = getContext("2d");
            ctx.fillStyle = Qt.rgba(1, 0, 0, 1);
            ctx.fillRect(0, 0, width, height);
        }
        onPainted: console.log('painted')

        onDataChanged: requestPaint()
    }

    Timer {
        interval: 100
        running: true
        repeat: true
        onTriggered: { cnv.requestPaint() }
    }
}