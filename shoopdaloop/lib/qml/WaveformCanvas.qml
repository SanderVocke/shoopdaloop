import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

Item {
    id: cnv
    property var waveform_data: []
    property var midi_data: []
    property int length_samples
    property int pixel_offset
    property real waveform_data_max : 1.0
    property real min_db: -60.0
    property real max_db: 0.0
    property bool dirty: false
    readonly property int max_canvas_width: 8192

    signal requestPaint()
    function makeDirty() { dirty = true }

    signal clicked (var mouse)

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

    Row {
        anchors.fill: parent
        spacing: 0

        Repeater {
            model: Math.ceil(length_samples / max_canvas_width)
            anchors {
                top: parent.top
                bottom: parent.bottom
            }

            Canvas {
                anchors {
                    top: parent.top
                    bottom: parent.bottom
                }
                width: cnv.max_canvas_width

                renderStrategy: Canvas.Threaded

                Connections {
                    target: cnv
                    function onRequestPaint() { requestPaint() }
                }
                onPaint: {
                    var ctx = getContext("2d");
                    ctx.reset()
                    ctx.fillStyle = Qt.rgba(0, 0, 0, 1);
                    ctx.fillRect(0, 0, width, height);

                    ctx.fillStyle = Qt.rgba(1, 0, 0, 1);
                    ctx.fillRect(0, height/2, width, 1);

                    var first_pixel =
                        (cnv.max_canvas_width * index) - cnv.pixel_offset;
                    for(var idx=0; idx < width; idx++) {
                        var pidx = idx + first_pixel
                        var db = (pidx >= 0 && pidx < waveform_data.length) ?
                            20.0 * Math.log(waveform_data[pidx] / waveform_data_max) / Math.log(10.0) :
                            0.0
                        var normalized = (Math.max(Math.min(db, max_db), min_db) - min_db) / (max_db - min_db)
                        ctx.fillRect(
                            idx,
                            Math.round((1.0-normalized)/2*height),
                            1,
                            Math.round(normalized*height)
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
        }
    }

    MouseArea {
        anchors.fill: parent
        onClicked: (event) => cnv.clicked(event)
    }
}
