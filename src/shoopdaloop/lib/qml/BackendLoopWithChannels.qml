import QtQuick 6.3
import QtQuick.Controls 6.3
import ShoopDaLoop.PythonLogger

import '../generated/types.js' as Types

// Wrap a Loop that may be dynamically loaded in a just-in-time way.
Item {
    id: root

    Loop {
        id: loop
    }

    readonly property PythonLogger logger : PythonLogger { name:"Frontend.Qml.BackendLoopWithChannels" }
    property alias mode: loop.mode
    property alias length: loop.length
    property alias position: loop.position
    property alias next_mode: loop.next_mode
    property alias next_transition_delay: loop.next_transition_delay
    property alias display_peaks: loop.display_peaks
    property alias display_midi_notes_active: loop.display_midi_notes_active
    property alias sync_source: loop.sync_source

    readonly property var maybe_backend_loop: loop

    property alias initial_descriptor: loop.initial_descriptor
    readonly property var audio_channel_descriptors: (initial_descriptor && initial_descriptor.channels) ? initial_descriptor.channels.filter(c => c.type == 'audio') : []
    readonly property var midi_channel_descriptors: (initial_descriptor && initial_descriptor.channels) ? initial_descriptor.channels.filter(c => c.type == 'midi') : []
    
    signal cycled();
    onParentChanged: (p) => loop.parentChanged(loop.parent)

    function initialize() {
        set_length(initial_descriptor.length)
    }

    Component.onCompleted: {
        if (loop.initialized) { initialize() }
        else { loop.initializedChanged.connect(initialize) }
    }
    Component.onDestruction: {
        loop.qml_close()
    }

    function get_audio_channels() {
        return loop.get_audio_channels();
    }

    function get_midi_channels() {
        return loop.get_midi_channels();
    }

    function add_audio_channel(mode) {
        return loop.add_audio_channel(mode)
    }

    function add_midi_channel(enabled) {
        return loop.add_midi_channel(enabled)
    }

    function load_audio_data(sound_channels) {
        loop.load_audio_data(sound_channels)
    }

    function clear(length) {
        loop.clear(length)
    }

    function transition(mode, delay, wait_for_sync) {
        loop.transition(mode, delay, wait_for_sync)
    }

    function update() {
        loop.update()
    }

    function set_length(length) {
        loop.set_length(length)
    }

    Repeater {
        id: audio_channels
        model: root.audio_channel_descriptors.length

        LoopAudioChannel {
            loop: loop
            descriptor: root.audio_channel_descriptors[index]
        }
    }
    Repeater {
        id: midi_channels
        model : root.midi_channel_descriptors.length

        LoopMidiChannel {
            loop: loop
            descriptor: root.midi_channel_descriptors[index]
        }
    }

    Connections {
        target: loop
        function onCycled() { root.cycled() }
    }
}