import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

Canvas {
    id: cnv
    property var data
    property real data_max
    property real min_db: 60.0
    property real pos: -1.0
    property real samples_per_pixel: 1.0

    onPaint: {
        var ctx = getContext("2d");
        ctx.reset()
        ctx.fillStyle = Qt.rgba(0, 0, 0, 1);
        ctx.fillRect(0, 0, width, height);

        ctx.fillStyle = Qt.rgba(1, 0, 0, 1);
        ctx.fillRect(0, height/2, width, 1);
        for(var idx=0; idx < data.length; idx++) {
            var db = 20.0 * Math.log(data[idx] / data_max) / Math.log(10.0)
            var normalized = Math.max(1.0 + (db / min_db), 0.0)
            ctx.fillRect(
                idx,
                (1.0-normalized)/2*height,
                1,
                normalized*height
            )
        }

        ctx.save()
    }
}