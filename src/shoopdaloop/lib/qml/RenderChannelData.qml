import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3
import ShoopDaLoop.PythonRenderAudioWaveform
import ShoopDaLoop.PythonRenderMidiSequence

Item {
    id: root
    property var input_data : []
    property real samples_per_bin : 1.0
    property int samples_offset : 0
    property var channel : null

    Loader {
        active: root.channel && root.channel.descriptor.type == "audio"
        sourceComponent: Component {
            PythonRenderAudioWaveform {
                input_data: root.input_data
                samples_per_bin: root.samples_per_bin
                samples_offset: root.samples_offset
                width: root.width
                height: root.height
            }
        }
    }

    Loader {
        active: root.channel && root.channel.descriptor.type == "midi"
        sourceComponent: Component {
            PythonRenderMidiSequence {
                messages: root.input_data
                samples_per_bin: root.samples_per_bin
                samples_offset: root.samples_offset
                width: root.width
                height: root.height
            }
        }
    }
}