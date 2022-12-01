import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15
import QtQuick.Dialogs

import '../../build/StatesAndActions.js' as StatesAndActions

// The loop widget displays the state of a single loop within a track.
Item {
    property bool is_master   // Master loop which everything syncs to in SL
    property bool is_in_selected_scene: false
    property bool is_in_hovered_scene: false
    property var manager
    property var port_manager
    property alias name: statusrect.name
    property string internal_name
    
    signal selected() //directly selected by the user to be activated.
    signal add_to_scene() //selected by the user to be added to the current scene.
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
    }

    component StatusRect : Rectangle {
        id: statusrect
        property var manager
        property bool hovered : area.containsMouse

        property string name

        signal propagateMousePosition(var point)
        signal propagateMouseExited()

        width: loop.width
        height: loop.height
        color: (manager && manager.state == StatesAndActions.LoopState.Empty) ? Material.background : '#000044'
        border.color: {
            var default_color = 'grey'
            if (!statusrect.manager) {
                return default_color;
            }

            if (widget.is_in_hovered_scene) {
                return 'blue';
            } else if (widget.is_in_selected_scene) {
                return 'red';
            }

            if (statusrect.manager.state == StatesAndActions.LoopState.Empty) {
                return default_color;
            }

            return '#DDDDDD' //blue'
        }
        border.width: 2

        MouseArea {
            id: area
            x: 0
            y: 0
            anchors.fill: parent
            //acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
            //onClicked: (event) => {
            //               if (event.button === Qt.LeftButton) { widget.selected() }
            //               else if (event.button === Qt.MiddleButton) { widget.add_to_scene() }
            //               else if (event.button === Qt.RightButton) { contextmenu.popup() }
            //           }
            hoverEnabled: true
            propagateComposedEvents: true
            onPositionChanged: (mouse) => { statusrect.propagateMousePosition(mapToGlobal(mouse.x, mouse.y)) }
            onExited: statusrect.propagateMouseExited()
        }

        Item {
            anchors.fill: parent
            anchors.margins: 2

            LoopProgressRect {
                height: parent.height
                anchors.left: parent.left
                anchors.right: parent.right
                y: 0
                manager: statusrect.manager
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
            width: 100
            height: 26
            x: 0
            y: 0

            Item {
                id: iconitem
                width: 24
                height: 24
                y: 0
                x: 0
                LoopStateIcon {
                    id: loopstateicon
                    state: statusrect.manager ? statusrect.manager.state : StatesAndActions.LoopState.Unknown
                    show_timer_instead: statusrect.manager ? statusrect.manager.state != statusrect.manager.next_state : false
                    connected: true
                    size: iconitem.height
                    y: 0
                    anchors.horizontalCenter: iconitem.horizontalCenter
                    onClicked: contextmenu.popup()
                }
                LoopStateIcon {
                    id: loopnextstateicon
                    state: statusrect.manager ? statusrect.manager.next_state : StatesAndActions.LoopState.Unknown
                    show_timer_instead: false
                    connected: true
                    size: iconitem.height * 0.65
                    y: 0
                    anchors.right : loopstateicon.right
                    anchors.bottom : loopstateicon.bottom
                    visible: statusrect.manager ? statusrect.manager.state != statusrect.manager.next_state : false
                }
            }

            Text {
                text: statusrect.name
                color: Material.foreground
                font.pixelSize: 11
                verticalAlignment: Text.AlignVCenter
                horizontalAlignment: Text.AlignLeft
                anchors.left: iconitem.right
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                anchors.right: parent.right
                anchors.rightMargin: 6
                visible: !buttongrid.visible
            }

            Grid {
                visible: statusrect.hovered || playlivefx.hovered || recordN.hovered || recordfx.hovered || recordn_menu.visible
                x: 20
                y: 2
                columns: 4
                id: buttongrid
                property int button_width: 18
                property int button_height: 22
                spacing: 1

                SmallButtonWithCustomHover {
                    id : play
                    width: buttongrid.button_width
                    height: buttongrid.button_height
                    MaterialDesignIcon {
                        size: parent.width
                        anchors.centerIn: parent
                        name: 'play'
                        color: 'green'
                    }

                    onClicked: { if(statusrect.manager) { statusrect.manager.doLoopAction(StatesAndActions.LoopActionType.DoPlay, 0.0, true) }}

                    ToolTip.delay: 1000
                    ToolTip.timeout: 5000
                    ToolTip.visible: hovered
                    ToolTip.text: "Play wet recording."

                    Connections {
                        target: statusrect
                        function onPropagateMousePosition(pt) { play.onMousePosition(pt) }
                        function onPropagateMouseExited() { play.onMouseExited() }
                    }

                    Popup {
                        background: Item{}
                        visible: play.hovered || ma.containsMouse
                        leftInset: 0
                        rightInset: 0
                        topInset: 0
                        bottomInset: 0
                        padding: 0
                        margins: 0

                        x: 0
                        y: play.height

                        Rectangle {
                            width: playlivefx.width
                            height: playlivefx.height
                            color: statusrect.color

                            MouseArea {
                                id: ma
                                x: 0
                                y: 0
                                width: parent.width
                                height: parent.height
                                hoverEnabled: true

                                onPositionChanged: (mouse) => { 
                                    var p = mapToGlobal(mouse.x, mouse.y)
                                    playlivefx.onMousePosition(p)
                                }
                                onExited: { playlivefx.onMouseExited() }
                            }

                            SmallButtonWithCustomHover {
                                id : playlivefx
                                width: buttongrid.button_width
                                height: buttongrid.button_height
                                IconWithText {
                                    size: parent.width
                                    anchors.centerIn: parent
                                    name: 'play'
                                    color: 'orange'
                                    text_color: Material.foreground
                                    text: "FX"
                                }
                                onClicked: { if(statusrect.manager) { statusrect.manager.doLoopAction(StatesAndActions.LoopActionType.DoPlayLiveFX, 0.0, true) }}

                                ToolTip.delay: 1000
                                ToolTip.timeout: 5000
                                ToolTip.visible: hovered
                                ToolTip.text: "Play dry recording through live effects. Allows hearing FX changes on-the-fly."
                            }
                        }
                    }
                }

                SmallButtonWithCustomHover {
                    id : record
                    width: buttongrid.button_width
                    height: buttongrid.button_height
                    MaterialDesignIcon {
                        size: parent.width
                        anchors.centerIn: parent
                        name: 'record'
                        color: 'red'
                    }

                    onClicked: { if(statusrect.manager) { statusrect.manager.doLoopAction(StatesAndActions.LoopActionType.DoRecord, 0.0, true) }}

                    ToolTip.delay: 1000
                    ToolTip.timeout: 5000
                    ToolTip.visible: hovered
                    ToolTip.text: "Trigger/stop recording. For master loop, starts immediately. For others, start/stop synced to next master loop cycle."

                    Connections {
                        target: statusrect
                        function onPropagateMousePosition(pt) { record.onMousePosition(pt) }
                        function onPropagateMouseExited() { record.onMouseExited() }
                    }

                    Popup {
                        background: Item{}
                        visible: record.hovered || ma_.containsMouse
                        leftInset: 0
                        rightInset: 0
                        topInset: 0
                        bottomInset: 0
                        padding: 0
                        margins: 0

                        x: 0
                        y: play.height

                        Rectangle {
                            width: recordN.width
                            height: recordN.height + recordfx.height
                            color: statusrect.color

                            MouseArea {
                                id: ma_
                                x: 0
                                y: 0
                                width: parent.width
                                height: parent.height
                                hoverEnabled: true

                                onPositionChanged: (mouse) => { 
                                    var p = mapToGlobal(mouse.x, mouse.y)
                                    recordN.onMousePosition(p)
                                    recordfx.onMousePosition(p)
                                }
                                onExited: { recordN.onMouseExited(); recordfx.onMouseExited() }
                            }

                            Column {
                                SmallButtonWithCustomHover {
                                    id : recordN
                                    property int n: 1
                                    width: buttongrid.button_width
                                    height: buttongrid.button_height
                                    IconWithText {
                                        size: parent.width
                                        anchors.centerIn: parent
                                        name: 'record'
                                        color: 'red'
                                        text_color: Material.foreground
                                        text: recordN.n.toString()
                                        font.pixelSize: size / 2.0
                                    }
                                    onClicked: { if(statusrect.manager) { statusrect.manager.doLoopAction(StatesAndActions.LoopActionType.DoRecordN, n, true) }}
                                    onPressAndHold: { recordn_menu.popup() }

                                    ToolTip.delay: 1000
                                    ToolTip.timeout: 5000
                                    ToolTip.visible: hovered
                                    ToolTip.text: "Trigger fixed-length recording. Length (number shown) is the amount of master loop cycles to record. Press and hold this button to change this number."

                                    // TODO: editable text box instead of fixed options
                                    Menu {
                                        id: recordn_menu
                                        title: 'Select # of cycles'
                                        MenuItem {
                                            text: "1"
                                            onClicked: () => { recordN.n = 1 }
                                        }
                                        MenuItem {
                                            text: "2"
                                            onClicked: () => { recordN.n = 2 }
                                        }
                                        MenuItem {
                                            text: "4"
                                            onClicked: () => { recordN.n = 4 }
                                        }
                                        MenuItem {
                                            text: "8"
                                            onClicked: () => { recordN.n = 8 }
                                        }
                                        MenuItem {
                                            text: "16"
                                            onClicked: () => { recordN.n = 16 }
                                        }
                                    }
                                }
                            
                                SmallButtonWithCustomHover {
                                    id : recordfx
                                    width: buttongrid.button_width
                                    height: buttongrid.button_height
                                    IconWithText {
                                        size: parent.width
                                        anchors.centerIn: parent
                                        name: 'record'
                                        color: 'orange'
                                        text_color: Material.foreground
                                        text: "FX"
                                    }
                                    onClicked: { if(statusrect.manager) { statusrect.manager.doLoopAction(StatesAndActions.LoopActionType.DoRecordFX, 0.0, true) }}

                                    ToolTip.delay: 1000
                                    ToolTip.timeout: 5000
                                    ToolTip.visible: hovered
                                    ToolTip.text: "Trigger FX re-record. This will play the full dry loop once with live FX, recording the result for wet playback."
                                }
                            }
                        }
                    }
                }

                SmallButtonWithCustomHover {
                    id : stop
                    width: buttongrid.button_width
                    height: buttongrid.button_height
                    MaterialDesignIcon {
                        size: parent.width
                        anchors.centerIn: parent
                        name: 'stop'
                        color: Material.foreground
                    }

                    onClicked: { if(statusrect.manager) { statusrect.manager.doLoopAction(StatesAndActions.LoopActionType.DoStop, 0.0, true) }}

                    ToolTip.delay: 1000
                    ToolTip.timeout: 5000
                    ToolTip.visible: hovered
                    ToolTip.text: "Stop."

                    Connections {
                        target: statusrect
                        function onPropagateMousePosition(pt) { stop.onMousePosition(pt) }
                        function onPropagateMouseExited() { stop.onMouseExited() }
                    }
                }
            }

            Item {
                anchors.right: parent.right
                width: 18
                height: 18
                anchors.verticalCenter: parent.verticalCenter
                anchors.rightMargin: 5

                // Display the volume dial always
                Dial {
                    anchors.fill: parent
                    from: 0.0
                    to:   1.0
                    signal internalValueChanged(real val)
                    property real externalValue: statusrect.manager ? statusrect.manager.volume : 1.0

                    onExternalValueChanged: { if (externalValue != value) {value = externalValue} }
                    onMoved: { statusrect.manager.volume = value }

                    inputMode: Dial.Vertical

                    handle.width: 4
                    handle.height: 4
                    
                    background: Rectangle {
                        radius: width / 2.0
                        width: parent.width
                        color: '#222222'
                        border.width: 1
                        border.color: 'grey'
                    }
                }
            }
        }
    }

    component LoopProgressRect : Item {
        id: loopprogressrect
        property var manager

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
            color: {
                var default_color = '#444444'
                if (!loopprogressrect.manager) {
                    return default_color;
                }

                switch(loopprogressrect.manager.state) {
                case StatesAndActions.LoopState.Playing:
                    return '#006600';
                case StatesAndActions.LoopState.PlayingLiveFX:
                    return '#555500';
                case StatesAndActions.LoopState.Recording:
                    return '#880000';
                case StatesAndActions.LoopState.RecordingFX:
                    return '#885500';
                default:
                    return default_color;
                }
            }
        }
    }

    component LoopStateIcon : Item {
        id: lsicon
        property int state
        property bool connected
        property bool show_timer_instead
        property int size
        property string description: StatesAndActions.LoopState_names[state] ? StatesAndActions.LoopState_names[state] : "Invalid"

        signal clicked()

        width: size
        height: size

        IconWithText {
            id: main_icon
            anchors.fill: parent

            name: {
                if(!lsicon.connected) {
                    return 'cancel'
                }
                if(lsicon.show_timer_instead) {
                    return 'timer-sand'
                }

                switch(lsicon.state) {
                case StatesAndActions.LoopState.Playing:
                case StatesAndActions.LoopState.PlayingLiveFX:
                    return 'play'
                case StatesAndActions.LoopState.PlayingMuted:
                    return 'volume-mute'
                case StatesAndActions.LoopState.Recording:
                case StatesAndActions.LoopState.RecordingFX:
                    return 'record-rec'
                case StatesAndActions.LoopState.Stopped:
                    return 'stop'
                case StatesAndActions.LoopState.Empty:
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
                case StatesAndActions.LoopState.Playing:
                    return '#00AA00'
                case StatesAndActions.LoopState.Recording:
                    return 'red'
                case StatesAndActions.LoopState.RecordingFX:
                case StatesAndActions.LoopState.PlayingLiveFX:
                    return 'orange'
                default:
                    return 'grey'
                }
            }

            text_color: Material.foreground
            text: {
                if(lsicon.show_timer_instead) {
                    return ''
                }
                switch(lsicon.state) {
                case StatesAndActions.LoopState.PlayingLiveFX:
                case StatesAndActions.LoopState.RecordingFX:
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
            acceptedButtons: Qt.RightButton | Qt.LeftButton
            onClicked: lsicon.clicked()
            id: ma
        }

        ToolTip {
            delay: 1000
            visible: ma.containsMouse
            text: description
        }
    }

    component ContextMenu: Item {
        property alias opened: menu.opened
        visible: menu.visible

        ClickTrackDialog {
            id: clicktrackdialog

            onAcceptedClickTrack: (filename) => {
                                    widget.manager.doLoadSoundFile(filename)
                                  }
        }

        Menu {
            id: menu
            title: 'Record'
            MenuItem {
                Row {
                    anchors.fill: parent
                    spacing: 5
                    anchors.leftMargin: 10
                    Text {
                        text: "Name:"
                        color: Material.foreground
                        anchors.verticalCenter: name_field.verticalCenter
                    }
                    TextField {
                        id: name_field
                        font.pixelSize: 12
                        onEditingFinished: {
                            widget.request_rename(text)
                            background_focus.forceActiveFocus();
                        }
                    }
                }
            }
            MenuItem {
                text: "Generate click loop..."
                onClicked: () => clicktrackdialog.open()
            }
            MenuItem {
                text: "Save dry to file..."
                onClicked: () => { savedialog.save_wet = false; savedialog.open() }
            }
            MenuItem {
                text: "Save wet to file..."
                onClicked: () => { savedialog.save_wet = true; savedialog.open() }
            }
            MenuItem {
                text: "Load from file..."
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
            property bool save_wet: true
            onAccepted: {
                var filename = selectedFile.toString().replace('file://', '');
                if (savedialog.save_wet) { widget.manager.doSaveWetToSoundFile(filename) }
                else { widget.manager.doSaveDryToSoundFile(filename) }
            }
        }

        FileDialog {
            id: loaddialog
            fileMode: FileDialog.OpenFile
            acceptLabel: 'Load'
            nameFilters: ["WAV files (*.wav)"]
            onAccepted: {
                var filename = selectedFile.toString().replace('file://', '');
                widget.manager.doLoadSoundFile(filename)
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
                        Text { text:  manager ? looper_details.manager.volume.toFixed(2) : ""; color: Material.foreground }
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

    component DryWetPairAbstractLooperManagerDetails : LooperManagerDetails {
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

            DryWetPairAbstractLooperManagerDetails {
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
            Text {
                color: Material.foreground
                text: 'rec. latency: ' + (widget.port_manager.recordingLatency * 1000.0).toFixed(1) + 'ms'
            }
        }
    }
}
