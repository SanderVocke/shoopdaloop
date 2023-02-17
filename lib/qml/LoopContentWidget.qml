import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

Item {
    id: widget
    property var backend_loop
    property int samples_per_waveform_pixel: 16
    property alias length_samples: waveform.length_samples
    property alias waveform_data: waveform.waveform_data
    property alias midi_data: waveform.midi_data
    property alias min_db: waveform.min_db
    property alias max_db: waveform.max_db
    property alias waveform_data_max: waveform.waveform_data_max
    property alias dirty: waveform.dirty
    property bool recording

    property bool updating: false

    function update_data() {
        updating = true
        waveform_data = {}
        var audio_channels = backend_loop.audio_channels
        for (var idx=0; idx < audio_channels.length; idx++) {
            waveform_data['audio channel ' + (idx+1).toString()] = audio_channels[idx].get_rms_data(0, backend_loop.length, samples_per_waveform_pixel)
        }
        console.log("warning: midi content data unimplemented")
        midi_data = {}
        updating = false
    }

    WaveformWidget {
        id: waveform
        anchors.fill: parent
    }

    Rectangle {
        color: 'green'
        width: 2
        height: widget.height
        x: widget.backend_loop.position / widget.backend_loop.length * widget.width
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
            dirty = true
        }
    }
}