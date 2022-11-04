import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15
import QtQuick.Dialogs

import '../../build/LoopState.js' as LoopState

// The loop widget displays the state of a single loop within a track.
Item {
    property bool is_selected // Selected by user
    property bool is_master   // Master loop which everything syncs to in SL
    property bool is_in_selected_scene: false
    property bool is_in_hovered_scene: false
    property var manager
    property alias name: statusrect.name
    property string internal_name
    
    signal selected() //directly selected by the user to be activated.
    signal add_to_scene() //selected by the user to be added to the current scene.
    signal request_load_wav(string wav_file) //request to load a wav into this loop
    signal request_save_wav(string wav_file)
    signal request_rename(string name)
    signal request_clear()

    // Internal
    id : widget

    width: childrenRect.width
    height: childrenRect.height
    clip: true

    // UI
    StatusRect {
        id: statusrect
        manager: widget.manager

        DetailsWindow {
            id: detailswindow
            manager: statusrect.manager
        }

        ContextMenu {
            id: contextmenu
        }

        MouseArea {
            x: 0
            y: 0
            z: statusrect.z - 1
            anchors {
                fill: parent
            }
            acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
            onClicked: (event) => {
                           if (event.button === Qt.LeftButton) { widget.selected() }
                           else if (event.button === Qt.MiddleButton) { widget.add_to_scene() }
                           else if (event.button === Qt.RightButton) { contextmenu.popup() }
                       }
        }
    }

    component StatusRect : Rectangle {
        id: statusrect
        property var manager
        property int x_spacing: 8
        property int y_spacing: 0

        property alias name: name_field.text

        width: loop.width + x_spacing
        height: loop.height + y_spacing
        color: widget.is_selected ? '#000044' : Material.background
        border.color: widget.is_in_hovered_scene && widget.is_in_selected_scene ? 'red' :
                      widget.is_in_hovered_scene ? 'blue' :
                      widget.is_in_selected_scene ? 'red' :
                      widget.is_selected ? Material.foreground :
                      'grey'
        border.width: 2

        Item {
            anchors.fill: parent
            anchors.margins: 2

            LoopProgressRect {
                height: parent.height
                anchors.left: parent.left
                anchors.right: parent.right
                y: 0
                manager: statusrect.manager
                is_selected: widget.is_selected
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
                Item {
                    id: iconitem
                    width: 28
                    height: 28
                    y: 3

                    LoopStateIcon {
                        id: loopstateicon
                        state: statusrect.manager ? statusrect.manager.state : LoopState.LoopState.Unknown
                        connected: true
                        size: iconitem.height
                        y: 0
                        anchors.horizontalCenter: iconitem.horizontalCenter
                    }
                }
                TextField {
                    id: name_field
                    width: 60
                    height: 35
                    font.pixelSize: 12
                    y: (loop.height - height)/2
                    z: statusrect.z + 1

                    onEditingFinished: {
                        widget.request_rename(text)
                        background_focus.forceActiveFocus();
                    }
                }
            }
        }
    }

    component LoopProgressRect : Item {
        id: loopprogressrect
        property var manager
        property bool is_selected

        Rectangle {
            function getRightMargin() {
                var st = loopprogressrect.manager
                if(st && st.length && st.length > 0) {
                    return (1.0 - (st.pos / st.length)) * parent.width
                }
                return parent.width
            }

            anchors {
                fill: parent
                rightMargin: getRightMargin()
            }
            color: is_selected ? '#550055' : '#444444'
        }
    }

    component LoopStateIcon : Item {
        id: lsicon
        property int state
        property bool connected
        property int size
        property string description: LoopState.LoopState_names[state] ? LoopState.LoopState_names[state] : "Invalid"

        width: size
        height: size

        IconWithText {
            id: main_icon
            anchors.fill: parent

            name: {
                if(!lsicon.connected) {
                    return 'cancel'
                }

                switch(lsicon.state) {
                case LoopState.LoopState.Playing:
                case LoopState.LoopState.PlayingLiveFX:
                    return 'play'
                case LoopState.LoopState.Recording:
                case LoopState.LoopState.RecordingFX:
                    return 'record-rec'
                case LoopState.LoopState.Stopped:
                    return 'stop'
                case LoopState.LoopState.Empty:
                    return 'border-none-variant'
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
                    return 'red'
                case LoopState.LoopState.RecordingFX:
                case LoopState.LoopState.PlayingLiveFX:
                    return 'orange'
                default:
                    return 'grey'
                }
            }

            text_color: Material.foreground
            text: {
                switch(lsicon.state) {
                case LoopState.LoopState.PlayingLiveFX:
                case LoopState.LoopState.RecordingFX:
                    return 'FX'
                default:
                    return ''
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
            MenuItem {
                text: "Loop details window"
                onClicked: () => { detailswindow.visible = true }
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

    component LooperManagerDetails : Item {
        id: looper_details
        property var manager
        property string title

        width: childrenRect.width
        height: childrenRect.height

        default property alias content: children_placeholder.children

        GroupBox {
            Column {
                spacing: 5

                Text { text: looper_details.title; color: Material.foreground }
                Row {
                    spacing: 5
                    StatusRect {
                        manager: looper_details.manager
                    }
                    Grid {
                        columns: 4
                        spacing: 3
                        horizontalItemAlignment: Grid.AlignRight
                        Text { text: 'vol:'; color: Material.foreground }
                        Text { text:  looper_details.manager.volume.toFixed(2); color: Material.foreground }
                        Text { text: 'panL:'; color: Material.foreground }
                        Text { text:  looper_details.manager.panL.toFixed(2); color: Material.foreground }
                        Text { text: 'panR:'; color: Material.foreground }
                        Text { text:  looper_details.manager.panR.toFixed(2); color: Material.foreground }
                    }
                    Item {
                        id: children_placeholder
                        width: childrenRect.width
                        height: childrenRect.height
                    }
                }
            }
        }
    }

    component BackendFXLooperPairManagerDetails : LooperManagerDetails {
    }
        

    component BackendLooperManagerDetails : LooperManagerDetails {

    }

    component DetailsWindow : ApplicationWindow {
        property var manager
        id: window
        title: widget.internal_name + ' details'

        width: 260
        height: 300
        minimumWidth: width
        maximumWidth: width
        minimumHeight: height
        maximumHeight: height
        
        Material.theme: Material.Dark

        Column {
            anchors.margins: 5
            anchors.centerIn: parent
            spacing: 5
            anchors.fill: parent

            BackendFXLooperPairManagerDetails {
                title: "Overall loop"
                manager: window.manager
            }
            BackendLooperManagerDetails {
                title: "Pre-FX loop"
                manager: window.manager.dry_looper
            }
            BackendLooperManagerDetails {
                title: "Post-FX loop"
                manager: window.manager.wet_looper
            }
        }
    }
}
