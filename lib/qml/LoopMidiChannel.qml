import LoopMidiChannel
import QtQuick 2.15

LoopMidiChannel {
    property int initial_mode
    onInitial_modeChanged: set_mode(initial_mode)
    Component.onCompleted: set_mode(initial_mode)
}