import QtQuick 2.15
import QtQuick.Controls 2.15

import '../backend/frontend_interface/types.js' as Types

// Wrap a Loop that may be dynamically loaded by a Loader in a just-in-time way.
Item {
    id: root

    readonly property bool ready : loader.status == Loader.Ready
    readonly property int mode : ready ? loader.item.mode : Types.LoopMode.Stopped
    readonly property int length : ready ? loader.item.length : 0
    readonly property int position : ready ? loader.item.position : 0
    readonly property int next_mode : ready ? loader.item.next_mode : Types.LoopMode.Stopped
    readonly property int next_transition_delay : ready ? loader.item.next_transition_delay : -1
    readonly property var maybe_loop : loader.item
    readonly property var display_peaks : ready ? loader.item.display_peaks : []
    readonly property int display_midi_notes_active : ready ? loader.item.display_midi_notes_active : 0
    default property alias contents : children_holder.children
    property bool force_load : false
    property alias loaded : loader.loaded

    signal backendLoopLoaded();
    signal cycled();

    // Careful: these bindings work only one-way (updating the internal loop).
    // If the value of the loop property changes, it will not be reflected back.
    property var sync_source : null

    function load() {
        loader.active = true
    }
    
    function qml_close() {
        if (loader.active) {
            loader.item.close();
        }
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

    function set_length(length) {
        load()
        loader.item.set_length(length)
    }

    Loader {
        id: loader
        active: root.force_load
        source: "Loop.qml"
        property bool loaded : !active || (ready && item.loaded)
        // property bool loaded : !active
        //     loaded = Qt.binding(function() { return !active || (status == Loader.Ready && item.loaded) });
        //     item.onLoadedChanged.connect(() => console.log("ITEM"))
        //     console.log("STATUS READY, ITEM LOADED", item.loaded, "LOADED", loaded);
        // }
    }

    // This will hold any children (typically Loop...Channels)
    Item {
        id: children_holder
    }

    Connections {
        target: loader
        function onLoaded() {
            loader.item.sync_source = Qt.binding(() => {
                return root.sync_source
            })
            // Reparent channels to the actual loop
            children_holder.parent = loader.item
            root.backendLoopLoaded()
        }
    }
    Connections {
        target: loader.item
        function onCycled() { root.cycled() }
    }
    onSync_sourceChanged: if(ready && loader.item) { loader.item.sync_source = sync_source }
}