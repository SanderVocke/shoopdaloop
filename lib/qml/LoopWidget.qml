import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15
import QtQuick.Dialogs

import '../LoopState.js' as LoopState

// The loop widget displays the state of a single loop within a track.
Item {
    property int loop_idx
    property var osc_link_obj
    property bool is_selected // Selected by user
    property bool is_master   // Master loop which everything syncs to in SL
    property bool is_in_selected_scene: false
    property bool is_in_hovered_scene: false
    property var manager
    property alias name: name_field.text

    signal selected() //directly selected by the user to be activated.
    signal add_to_scene() //selected by the user to be added to the current scene.
    signal state_changed()
    signal request_load_wav(string wav_file) //request to load a wav into this loop
    signal request_save_wav(string wav_file)
    signal request_rename(string name)
    signal request_clear()

    id : widget

    width: childrenRect.width
    height: childrenRect.height
    clip: true

    // UI
    Rectangle {
        property int x_spacing: 8
        property int y_spacing: 0

        width: loop.width + x_spacing
        height: loop.height + y_spacing
        color: widget.is_selected ? '#000044' : Material.background
        border.color: widget.is_in_hovered_scene && widget.is_in_selected_scene ? 'red' :
                      widget.is_in_hovered_scene ? 'blue' :
                      widget.is_in_selected_scene ? 'red' :
                      widget.is_selected ? Material.foreground :
                      'grey'
        border.width: 2

        // This rectangle shows the playback progress for running loops.
        Item {
            anchors.fill: parent
            anchors.margins: 2

            Rectangle {
                function getRightMargin() {
                    var st = widget.manager
                    if(st.length && st.length > 0) {
                        return (1.0 - (st.pos / st.length)) * parent.width
                    }
                    return parent.width
                }

                anchors {
                    fill: parent
                    rightMargin: getRightMargin()
                }
                color: widget.is_selected ? '#550055' : '#444444'
            }
        }

        ContextMenu {
            id: contextmenu
        }

        MouseArea {
            x: 0
            y: 0
            width: loop.width + parent.x_spacing
            height: loop.height + parent.y_spacing
            acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
            onClicked: (event) => {
                           if (event.button === Qt.LeftButton) { widget.selected() }
                           else if (event.button === Qt.MiddleButton) { widget.add_to_scene() }
                           else if (event.button === Qt.RightButton) { contextmenu.popup() }
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
                    state: widget.manager.state
                    connected: widget.manager.connected
                    size: 24
                    y: (loop.height - height)/2
                }
                TextField {
                    id: name_field
                    width: 60
                    height: 35
                    font.pixelSize: 12
                    y: (loop.height - height)/2

                    onEditingFinished: {
                        console.log("Finished: " + text)
                        widget.request_rename(text)
                        background_focus.forceActiveFocus();
                    }
                }
            }
        }
    }

    component LoopStateIcon : Item {
        id: lsicon
        property int state
        property bool connected
        property int size
        property string description: LoopState.LoopStateDesc[state] ? LoopState.LoopStateDesc[state] : "Invalid"

        width: size
        height: size

        MaterialDesignIcon {
            id: main_icon
            anchors.fill: parent

            name: {
                if(!lsicon.connected) {
                    return 'cancel'
                }

                switch(lsicon.state) {
                case LoopState.LoopState.Playing:
                    return 'play'
                case LoopState.LoopState.Recording:
                case LoopState.LoopState.Inserting:
                    return 'record-rec'
                case LoopState.LoopState.Paused:
                    return 'pause'
                case LoopState.LoopState.Muted:
                    return 'volume-mute'
                case LoopState.LoopState.WaitStart:
                case LoopState.LoopState.WaitStop:
                    return 'timer-sand'
                case LoopState.LoopState.Off:
                    return 'stop'
                default:
                    return 'help-circle'
                }
            }

            color: {
                if(!lsicon.connected) {
                    return 'grey'
                }
                switch(lsicon.state) {
                case LoopState.LoopState.Playing:
                    return 'green'
                case LoopState.LoopState.Recording:
                case LoopState.LoopState.Inserting:
                    return 'red'
                default:
                    return 'grey'
                }
            }

            size: parent.size
        }

        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            propagateComposedEvents: true
            id: ma
        }

        ToolTip {
            delay: 1000
            visible: ma.containsMouse
            text: description
        }
    }

    component ContextMenu: Item {
        ClickTrackDialog {
            id: clicktrackdialog
            onAcceptedClickTrack: (wav_file) => {
                                      widget.request_load_wav(wav_file)
                                  }
        }

        Menu {
            id: menu
            title: 'Record'
            MenuItem {
                text: "Generate click loop..."
                onClicked: () => clicktrackdialog.open()
            }
            MenuItem {
                text: "Save WAV..."
                onClicked: () => savedialog.open()
            }
            MenuItem {
                text: "Load WAV..."
                onClicked: () => loaddialog.open()
            }
            MenuItem {
                text: "Clear"
                onClicked: () => {
                               widget.request_clear()
                           }
            }
        }

        FileDialog {
            id: savedialog
            fileMode: FileDialog.SaveFile
            acceptLabel: 'Save'
            nameFilters: ["WAV files (*.wav)"]
            defaultSuffix: 'wav'
            onAccepted: {
                var filename = selectedFile.toString().replace('file://', '');
                widget.request_save_wav(filename)
            }
        }

        FileDialog {
            id: loaddialog
            fileMode: FileDialog.OpenFile
            acceptLabel: 'Load'
            nameFilters: ["WAV files (*.wav)"]
            onAccepted: {
                var filename = selectedFile.toString().replace('file://', '');
                widget.request_load_wav(filename)
            }

        }

        function popup () {
            menu.popup()
        }
    }
}
