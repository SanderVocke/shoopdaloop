import QtQuick 2.15
import QtQuick.Controls 2.15

import '../../build/types.js' as Types

// Wrap a Loop that may be dynamically loaded by a Loader in a just-in-time way.
Item {
    id: item

    readonly property bool ready : loader.status == Loader.Ready
    readonly property int mode : ready ? loader.item.mode : Types.LoopMode.Stopped
    readonly property int length : ready ? loader.item.length : 0
    readonly property int position : ready ? loader.item.position : 0
    readonly property int next_mode : ready ? loader.item.next_mode : Types.LoopMode.Stopped
    readonly property int next_transition_delay : ready ? loader.item.next_transition_delay : -1
    readonly property var maybe_loop : loader.item
    default property alias contents : children_holder.children

    property bool force_load : false

    // Careful: these bindings work only one-way (updating the internal loop).
    // If the value of the loop property changes, it will not be reflected back.
    property var sync_source : null

    function load() {
        loader.active = true
    }

    function audio_channels() {
        if (!ready) { return []; }
        return loader.item.audio_channels();
    }

    function midi_channels() {
        if (!ready) { return []; }
        return loader.item.midi_channels();
    }

    function add_audio_channel(mode) {
        load()
        return loader.item.add_audio_channel(mode)
    }

    function add_midi_channel(enabled) {
        load()
        return loader.item.add_midi_channel(enabled)
    }

    function load_audio_data(sound_channels) {
        load()
        loader.item.load_audio_data(sound_channels)
    }

    function load_audio_file(filename, force_length, forced_length) {
        load()
        loader.item.load_audio_file(filename, force_length, forced_length)
    }

    function clear(length) {
        load()
        loader.item.clear(length)
    }

    function transition(mode, delay, wait_for_sync) {
        load()
        loader.item.transition(mode, delay, wait_for_sync)
    }

    function update() {
        if (ready) { loader.item.update() }
    }

    Loader {
        id: loader
        active: item.force_load
        source: "Loop.qml"
    }

    // This will hold any children (typically Loop...Channels), and reparent
    // to go inside the loaded Loop once loaded.
    Item {
        id: children_holder

        Connections {
            target: loader
            function onLoaded() {
                console.log('LOADED LOOP', item.sync_source)
                loader.item.sync_source = Qt.binding(() => {
                    return item.sync_source
                })
            }
        }
    }
}