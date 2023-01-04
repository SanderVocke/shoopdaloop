import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

Item {
    id: widget
    property var manager
    property int samples_per_pixel: 16
    property alias waveform_data: waveform.waveform_data
    property alias min_db: waveform.min_db
    property alias max_db: waveform.max_db
    property alias waveform_data_max: waveform.waveform_data_max
    property alias dirty: waveform.dirty

    //onSamples_per_pixelChanged: { update_data() }

    layer.enabled: true
    layer.samples: 4

    function update_data() {
        var waveforms = manager.get_waveforms(0, manager.length, samples_per_pixel)
        var entry = Object.entries(waveforms)[0]
        waveform_data = entry[1]
    }

    WaveformWidget {
        id: waveform
        anchors.fill: parent
    }

    Rectangle {
        color: 'green'
        width: 2
        height: widget.height
        x: widget.manager.pos / widget.manager.length * widget.width
        y: 0
    }

    MouseArea {
        anchors.fill: parent
        anchors.bottomMargin: 20
        onClicked: {
            waveform.update_data()
        }
    }
}