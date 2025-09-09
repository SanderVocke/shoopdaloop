import ShoopDaLoop.Rust
import QtQuick 6.6

ShoopRustLoopGui {
    id: root
    property bool loaded : initialized

    RequireBackend {}

    readonly property ShoopRustLogger logger: ShoopRustLogger {
        name: "Frontend.Qml.Loop"
        instance_identifier: root.instance_identifier
    }
    instance_identifier: obj_id

    property var maybe_fx_chain: null
    property var loop_widget : null

    // Gives a nice full progress bar when recording
    readonly property int display_position: mode == ShoopRustConstants.LoopMode.Recording ? length : position

    property var initial_descriptor: null
    property string obj_id: initial_descriptor ? initial_descriptor.id : null
}