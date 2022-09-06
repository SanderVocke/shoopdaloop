import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

import '../LoopState.js' as LoopState

import SLLooperState 1.0
import SLLooperManager 1.0

// The loop widget displays the state of a single loop within a track.
Item {
    property int loop_idx
    property var osc_link_obj
    property bool is_selected // Selected by user
    property bool is_master   // Master loop which everything syncs to in SL
    property bool is_in_selected_scene: false
    property bool is_in_hovered_scene: false
    property var manager: looper_mgr
    property var state_mgr: looper_state

    signal selected() //directly selected by the user to be activated.
    signal add_to_scene() //selected by the user to be added to the current scene.

    id : widget

    width: childrenRect.width
    height: childrenRect.height
    clip: true

    // State and OSC management
    SLLooperState {
        id: looper_state
    }
    SLLooperManager {
        id: looper_mgr
        sl_looper_index: loop_idx
    }

    // Initialization
    Component.onCompleted: {
        looper_state.connect_manager(looper_mgr)
        looper_mgr.connect_osc_link(osc_link)
        looper_mgr.start_sync()
    }

    // UI
    Rectangle {
        property int x_spacing: 8
        property int y_spacing: 0

        width: loop.width + x_spacing
        height: loop.height + y_spacing
        color: widget.is_selected ? Material.accent : Material.background
        border.color: widget.is_in_hovered_scene ? 'blue' :
                      widget.is_in_selected_scene ? 'red' :
                      widget.is_selected ? Material.foreground :
                      'grey'
        border.width: 2

        MouseArea {
            x: 0
            y: 0
            width: loop.width + parent.x_spacing
            height: loop.height + parent.y_spacing
            acceptedButtons: Qt.LeftButton | Qt.MiddleButton
            onClicked: (event) => {
                           if (event.button === Qt.LeftButton) { widget.selected() }
                           else if (event.button === Qt.MiddleButton) {widget.add_to_scene() }
                       }
        }

        MaterialDesignIcon {
            size: 10
            name: 'star'
            color: "yellow"

            anchors {
                left: parent.left
                leftMargin: 2
                top: parent.top
                topMargin: 2
            }

            visible: widget.is_master
        }

        Item {
            id : loop
            width: childrenRect.width
            height: childrenRect.height
            x: parent.x_spacing/2
            y: parent.y_spacing/2

            Row {
                spacing: 5
                LoopStateIcon {
                    id: loopstateicon
                    state: looper_state.state
                    connected: looper_state.connected
                    size: 24
                    y: (loop.height - height)/2
                }
                TextField {
                    width: 60
                    height: 35
                    font.pixelSize: 12
                    y: (loop.height - height)/2

                    onEditingFinished: background_focus.forceActiveFocus()
                }
            }
        }
    }

    component LoopStateIcon : MaterialDesignIcon {
        property int state
        property bool connected
        name: 'help-circle'
        color: 'red'

        function getName() {
            if(!connected) {
                return 'cancel'
            }

            switch(state) {
            case LoopState.LoopState.Playing:
                return 'play'
            case LoopState.LoopState.Recording:
                return 'record-rec'
            case LoopState.LoopState.Paused:
                return 'pause'
            default:
                return 'help-circle'
            }
        }

        function getColor() {
            if(!connected) {
                return 'grey'
            }

            switch(state) {
            case LoopState.LoopState.Playing:
                return 'green'
            case LoopState.LoopState.Recording:
                return 'red'
            case LoopState.LoopState.Paused:
                return 'black'
            default:
                return 'grey'
            }
        }

        Component.onCompleted: {
            name = Qt.binding(getName);
            color = Qt.binding(getColor);
        }
    }
}
