import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6

Item {
    id: root
    property var waveform_data: []
    property var midi_notes: []
    readonly property int data_length : {
        var audio = waveform_data ? waveform_data.length : 0
        var midi = midi_notes && midi_notes.length > 0 ? midi_notes[midi_notes.length - 1]['end'] : 0
        return Math.max(audio, midi)
    }
    width: Math.max(1, data_length)
    property real waveform_data_max : 1.0
    property real min_db: -60.0
    property real max_db: 0.0
    property bool dirty: false
    readonly property int max_canvas_width: 8192

    clip: true

    signal requestPaint()
    function makeDirty() { dirty = true }

    onVisibleChanged: {
        if (visible && dirty) { requestPaint() }
    }

    onDirtyChanged: {
        if (visible) { requestPaint() }
    }

    onWaveform_dataChanged: dirty = true
    onMidi_notesChanged: dirty = true
    onWaveform_data_maxChanged: dirty = true
    onMin_dbChanged: dirty = true
    onMax_dbChanged: dirty = true
    onWidthChanged: dirty = true
    onHeightChanged: dirty = true

    Row {
        anchors.fill: parent
        spacing: 0

        // At the time of writing this, there is a bug in Qt (https://bugreports.qt.io/browse/QTBUG-51530)
        // which renders blurry if a canvas is too large.
        // Therefore, we render multiple canvases if needed.
        Repeater {
            model: Math.ceil(root.data_length / max_canvas_width)
            anchors {
                top: parent.top
                bottom: parent.bottom
            }

            Canvas {
                anchors {
                    top: parent.top
                    bottom: parent.bottom
                }
                width: root.max_canvas_width

                renderStrategy: Canvas.Threaded

                Connections {
                    target: root
                    function onRequestPaint() { requestPaint() }
                }

                function clamp(x, min, max) {
                    return Math.max(Math.min(x, max), min)
                }

                onPaint: {
                    var ctx = getContext("2d");
                    ctx.reset()

                    if (waveform_data && waveform_data.length > 0) {
                        ctx.fillStyle = Qt.rgba(1, 0, 0, 1);
                        ctx.fillRect(0, height/2, width, 1);
                    }

                    var first_pixel =
                        (root.max_canvas_width * index);
                    var last_pixel =
                        first_pixel + root.max_canvas_width

                    // Draw audio waveform data
                    for(var idx=0; idx < width; idx++) {
                        var pidx = idx + first_pixel
                        var db = (pidx >= 0 && pidx < waveform_data.length) ?
                            20.0 * Math.log(waveform_data[pidx] / waveform_data_max) / Math.log(10.0) :
                            min_db
                        var normalized = (Math.max(Math.min(db, max_db), min_db) - min_db) / (max_db - min_db)
                        ctx.fillRect(
                            idx,
                            Math.round((1.0-normalized)/2*height),
                            1,
                            Math.round(normalized*height)
                        )
                    }

                    // Draw MIDI data
                    ctx.fillStyle = Qt.rgba(1, 1, 1, 1);
                    var note_height = (height * 4) / 128;
                    for(var note of midi_notes) {
                        if (note.start < last_pixel || note.end >= first_pixel) {
                            var x_start = clamp(note.start - first_pixel, first_pixel, root.max_canvas_width - 1)
                            var x_end = clamp(note.end - first_pixel, first_pixel, root.max_canvas_width - 1)
                            var note_width = x_end - x_start
                            var y = (height-note_height) - (((height-2*note_height) / 128) * note.note)

                            ctx.fillRect(
                                x_start,
                                y,
                                note_width,
                                note_height
                            )
                        }
                    }

                    dirty=false
                }
            }
        }
    }
}
