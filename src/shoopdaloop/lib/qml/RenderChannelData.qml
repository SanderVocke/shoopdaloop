import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6
import ShoopDaLoop.PythonRenderAudioWaveform
import ShoopDaLoop.PythonRenderMidiSequence

Item {
    id: root
    property var input_data : null
    property real samples_per_bin : 1.0
    property int samples_offset : 0
    property var channel : null

    property bool is_audio : root.channel && root.channel.descriptor.type == "audio"
    property bool is_midi : root.channel && root.channel.descriptor.type == "midi"

    PythonRenderAudioWaveform {
        input_data: is_audio ? root.input_data : null
        samples_per_bin: root.samples_per_bin
        samples_offset: root.samples_offset
        width: root.width
        height: root.height
    }

    PythonRenderMidiSequence {
        messages: is_midi ? root.input_data : null
        samples_per_bin: root.samples_per_bin
        samples_offset: root.samples_offset
        width: root.width
        height: root.height
    }
}