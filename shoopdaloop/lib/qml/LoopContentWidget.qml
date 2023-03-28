import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

Item {
    id: widget
    property var loop
    property int samples_per_waveform_pixel: 16
    property int length_samples: 0
    property var waveform_data : {}
    property var midi_data
    //property alias midi_data: waveform.midi_data
    property real min_db: 0.0
    property real max_db: 0.0
    //property real waveform_data_max: 0.0
    //property alias dirty: waveform.dirty
    property bool recording
    property bool updating: false

    function update_data() {
        updating = true
        var _data = {}
        var audio_channels = loop.audio_channels()
        var max = 0.0
        for (var idx=0; idx < audio_channels.length; idx++) {
            _data['audio channel ' + (idx+1).toString()] = audio_channels[idx].get_rms_data(0, loop.length, samples_per_waveform_pixel)
        }
        console.log("warning: midi content data unimplemented")
        midi_data = {}
        waveform_data = _data
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

    Column {
        anchors.top: toolbar.bottom
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

                WaveformWidget {
                    id: waveform
                    waveform_data: (widget.waveform_data && Object.entries(widget.waveform_data).length > index) ?
                        Object.entries(widget.waveform_data)[index][1] : []
                    anchors.fill: parent

                    length_samples: widget.length_samples
                    min_db : widget.min_db
                    max_db : widget.max_db
                }

                Rectangle {
                    color: 'green'
                    width: 2
                    height: parent.height
                    x: widget.loop ? widget.loop.position / widget.loop.length * widget.width : 0
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