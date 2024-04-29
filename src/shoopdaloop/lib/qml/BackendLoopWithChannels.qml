import QtQuick 6.6
import QtQuick.Controls 6.6
import ShoopDaLoop.PythonLogger
import ShoopDaLoop.PythonLoop

import ShoopConstants

// Wrap a Loop that may be dynamically loaded in a just-in-time way.
Loop {
    id: root
    readonly property var loop : root
    readonly property var maybe_backend_loop: root

    readonly property PythonLogger logger : PythonLogger { name:"Frontend.Qml.BackendLoopWithChannels" }

    readonly property var audio_channel_descriptors: (initial_descriptor && initial_descriptor.channels) ? initial_descriptor.channels.filter(c => c.type == 'audio') : []
    readonly property var midi_channel_descriptors: (initial_descriptor && initial_descriptor.channels) ? initial_descriptor.channels.filter(c => c.type == 'midi') : []

    function initialize() {
        set_length(initial_descriptor.length)
    }

    Component.onCompleted: {
        if (initialized) { initialize() }
        else { initializedChanged.connect(initialize) }
    }
    Component.onDestruction: {
        qml_close()
    }

    function is_all_empty() {
        return length == 0 && audio_channels.every(c => c.is_empty()) && midi_channels.every(c => c.is_empty())
    }

    Mapper {
        id: audio_channels_mapper
        model: root.audio_channel_descriptors

        LoopAudioChannel {
            property var mapped_item
            property int index
            loop: root.loop
            descriptor: mapped_item
        }
    }
    Mapper {
        id: midi_channels_mapper
        model : root.midi_channel_descriptors

        LoopMidiChannel {
            property var mapped_item
            property int index
            loop: root.loop
            descriptor: mapped_item
        }
    }

    property alias audio_channels : audio_channels_mapper.unsorted_instances
    property alias midi_channels: midi_channels_mapper.unsorted_instances
}