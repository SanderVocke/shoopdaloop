import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

Item {
    id: widget
    property var loop
    property int samples_per_waveform_pixel: 16
    property int length_samples: 0
    property int start_idx: 0
    property var waveform_data : {}
    property var midi_data : {}
    //property alias midi_data: waveform.midi_data
    property real min_db: 0.0
    property real max_db: 0.0
    //property real waveform_data_max: 0.0
    //property alias dirty: waveform.dirty
    property bool recording
    property bool updating: false

    function update_data() {
        updating = true
        var audio_data = {}
        var audio_channels = loop.audio_channels()
        var midi_channels = loop.midi_channels()
        var min_pos = 0
        var max_pos = loop.length
        for (var idx=0; idx < audio_channels.length; idx++) {
            var chan = audio_channels[idx]
            var data = chan.get_rms_data(0, loop.length, samples_per_waveform_pixel)
            min_pos = Math.min(min_pos, -chan.start_offset)
            max_pos = Math.max(max_pos, chan.data_length)

            var key = 'audio channel ' + (idx+1).toString()
            audio_data[key] = {}
            audio_data[key]['data'] = data
            audio_data[key]['start_offset'] = chan.start_offset
        }
        console.log("warning: midi content data unimplemented")
        midi_data = {}
        start_idx = min_pos
        length_samples = (max_pos - min_pos)
        waveform_data = audio_data
        updating = false
        waveform_dataChanged()
    }

    Row {
        id: toolbar
        anchors {
            top: parent.top
            left: parent.left
        }
        height: 40

        Label {
            text: "Hello World"
        }
    }

    Slider {
        id: start_offset_slider
        anchors {
            top: toolbar.bottom
            left: parent.left
            right: parent.right
        }
    }

    Column {
        anchors.top: start_offset_slider.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        spacing: 4

        Repeater {        

            model : {
                console.log("Waveform data:", widget.waveform_data, Object.keys(widget.waveform_data))
                Object.keys(widget.waveform_data).length
            }

            Item {
                anchors {
                    left: parent.left
                    right: parent.right
                }
                height: 80

                property var _waveform_data : (widget.waveform_data && Object.entries(widget.waveform_data).length > index) ?
                        Object.values(widget.waveform_data)[index]['data'] : []
                property int _start_offset : (widget.waveform_data && Object.entries(widget.waveform_data).length > index) ?
                        Object.values(widget.waveform_data)[index]['start_offset'] : 0
                property int display_length : widget.length_samples

                function normalize(pos) {
                    return (pos * widget.width) / display_length
                }

                WaveformWidget {
                    id: waveform
                    anchors.fill: parent

                    waveform_data: parent._waveform_data
                    length_samples: parent.display_length
                    start_offset: parent._start_offset
                    min_db : widget.min_db
                    max_db : widget.max_db
                }

                Rectangle {
                    color: 'blue'
                    width: 2
                    height: parent.height
                    x: widget.loop ? parent.normalize(parent._start_offset) : 0
                    y: 0
                }

                Rectangle {
                    color: 'green'
                    width: 2
                    height: parent.height
                    x: widget.loop ? parent.normalize(widget.loop.position + parent._start_offset) : 0
                    y: 0
                }

                Rectangle {
                    color: 'blue'
                    anchors.fill: parent
                    visible: updating
                }

                Label {
                    anchors.fill: parent
                    background: Rectangle { color: 'black' }
                    visible: recording
                    color: Material.foreground
                    text: "Recording..."
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }

                MouseArea {
                    anchors.fill: parent
                    anchors.bottomMargin: 20
                    onClicked: {
                        update_data()
                        waveform.dirty = true
                    }
                }
            }
        }
    }
}