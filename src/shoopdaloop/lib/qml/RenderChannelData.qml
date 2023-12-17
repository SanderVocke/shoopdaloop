import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3
import ShoopDaLoop.PythonRenderAudioWaveform
import ShoopDaLoop.PythonRenderMidiSequence

Item {
    property var input_data
    property real samples_per_bin
    property int samples_offset
    property var channel

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