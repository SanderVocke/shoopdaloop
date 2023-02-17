import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

Item {
    id: widget
    property var manager
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
        var waveforms = manager.get_waveforms(0, manager.length, samples_per_waveform_pixel)
        var entry = Object.entries(waveforms)[0]
        waveform_data = entry[1]
        midi_data = manager.get_midi()
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
        x: widget.manager.position / widget.manager.length * widget.width
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