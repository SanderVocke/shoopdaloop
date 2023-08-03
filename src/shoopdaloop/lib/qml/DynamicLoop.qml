import QtQuick 6.3
import QtQuick.Controls 6.3
import ShoopDaLoop.PythonLogger

import '../generated/types.js' as Types

// Wrap a Loop that may be dynamically loaded in a just-in-time way.
Item {
    id: root

    property var maybe_loop : null
    readonly property PythonLogger logger : PythonLogger { name:"Frontend.DynamicLoop" }
    readonly property bool ready : maybe_loop != null
    readonly property int mode : ready ? maybe_loop.mode : Types.LoopMode.Stopped
    readonly property int length : ready ? maybe_loop.length : 0
    readonly property int position : ready ? maybe_loop.position : 0
    readonly property int next_mode : ready ? maybe_loop.next_mode : Types.LoopMode.Stopped
    readonly property int next_transition_delay : ready ? maybe_loop.next_transition_delay : -1
    readonly property var display_peaks : ready ? maybe_loop.display_peaks : []
    readonly property int display_midi_notes_active : ready ? maybe_loop.display_midi_notes_active : 0
    default property alias contents : children_holder.children
    property bool force_load : false
    
    signal backendLoopLoaded();
    signal cycled();

    // Careful: these bindings work only one-way (updating the internal loop).
    // If the value of the loop property changes, it will not be reflected back.
    property var sync_source : null

    readonly property var factory : Qt.createComponent("Loop.qml")

    function load() {
        if (maybe_loop) { return }
        if (factory.status == Component.Error) {
            throw new Error("TracksWidget: Failed to load track factory: " + factory.errorString())
        } else if (factory.status != Component.Ready) {
            throw new Error("TracksWidget: Track factory not ready")
        } else {
            var loop = factory.createObject(root, {})
            loop.sync_source = Qt.binding(() => {
                return root.sync_source
            })
            // Reparent channels to the actual loop
            children_holder.parent = loop
            logger.debug("Loaded back-end loop")
            maybe_loop = loop
            root.backendLoopLoaded()
        }
    }
    Component.onCompleted: if(force_load) load()
    onForce_loadChanged: if(force_load) load()
    
    function qml_close() {
        if (maybe_loop) {
            maybe_loop.close();
        }
    }

    function audio_channels() {
        if (!ready) { return []; }
        return maybe_loop.audio_channels();
    }

    function midi_channels() {
        if (!ready) { return []; }
        return maybe_loop.midi_channels();
    }

    function add_audio_channel(mode) {
        load()
        return maybe_loop.add_audio_channel(mode)
    }

    function add_midi_channel(enabled) {
        load()
        return maybe_loop.add_midi_channel(enabled)
    }

    function load_audio_data(sound_channels) {
        load()
        maybe_loop.load_audio_data(sound_channels)
    }

    function clear(length) {
        load()
        maybe_loop.clear(length)
    }

    function transition(mode, delay, wait_for_sync) {
        load()
        maybe_loop.transition(mode, delay, wait_for_sync)
    }

    function update() {
        if (ready) { maybe_loop.update() }
    }

    function set_length(length) {
        if (!maybe_loop) {
            logger.debug("Setting length to " + length + " before loop is loaded")
            logger.debug((new Error).stack)
        }
        load()
        maybe_loop.set_length(length)
    }

    // This will hold any children (typically Loop...Channels)
    Item {
        id: children_holder
    }

    Connections {
        target: maybe_loop
        function onCycled() { root.cycled() }
    }
    onSync_sourceChanged: if(maybe_loop) { maybe_loop.sync_source = sync_source }
}