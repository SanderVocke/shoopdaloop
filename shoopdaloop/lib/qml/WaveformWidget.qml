import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

Canvas {
    id: cnv
    property var waveform_data: []
    property var midi_data: []
    property int length_samples
    property real waveform_data_max
    property real min_db: -60.0
    property real max_db: 0.0
    property bool dirty: false

    function makeDirty() { dirty = true }

    onVisibleChanged: {
        if (visible && dirty) { requestPaint() }
    }

    onDirtyChanged: {
        if (visible) { requestPaint() }
    }

    onWaveform_dataChanged: dirty = true
    onMidi_dataChanged: dirty = true
    onWaveform_data_maxChanged: dirty = true
    onMin_dbChanged: dirty = true
    onMax_dbChanged: dirty = true
    onWidthChanged: dirty = true
    onHeightChanged: dirty = true
    onLength_samplesChanged: dirty = true
    
    onPaint: {
        console.log("Paint data", waveform_data.length, waveform_data)
        var ctx = getContext("2d");
        ctx.reset()
        ctx.fillStyle = Qt.rgba(0, 0, 0, 1);
        ctx.fillRect(0, 0, width, height);

        ctx.fillStyle = Qt.rgba(1, 0, 0, 1);
        ctx.fillRect(0, height/2, width, 1);
        for(var idx=0; idx < waveform_data.length; idx++) {
            var db = 20.0 * Math.log(waveform_data[idx] / waveform_data_max) / Math.log(10.0)
            var normalized = (Math.max(Math.min(db, max_db), min_db) - min_db) / (max_db - min_db)
            ctx.fillRect(
                idx,
                (1.0-normalized)/2*height,
                1,
                normalized*height
            )
        }

        ctx.fillStyle = Qt.rgba(0, 1, 1, 1);
        for(var idx=0; idx < midi_data.length; idx++) {
            ctx.fillRect(
                midi_data[idx]['time'] / length_samples * width,
                0,
                1,
                height
            )
        }

        dirty=false
    }
}
