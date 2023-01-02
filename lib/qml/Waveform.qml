import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

Canvas {
    id: cnv
    property var data
    property real max

    //contextType: "2d"
    onPaint: {
        var ctx = getContext("2d");
        ctx.reset()
        ctx.fillStyle = Qt.rgba(0, 0, 0, 1);
        ctx.fillRect(0, 0, width, height);

        ctx.fillStyle = Qt.rgba(1, 0, 0, 1);
        ctx.fillRect(0, height/2, width, 1);
        for(var idx=0; idx < data.length; idx++) {
            var db = 20.0 * Math.log(data[idx] / max) / Math.log(10.0)
            var normalized = Math.max(1.0 + (db / 60.0), 0.0)
            ctx.fillRect(
                idx,
                (1.0-normalized)/2*height,
                1,
                normalized*height
            )
            console.log(db, normalized)
        }
    }
}