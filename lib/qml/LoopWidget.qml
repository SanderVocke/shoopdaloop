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
    property var master_manager
    property var ports_manager
    property alias name: statusrect.name
    property string internal_name

    property int n_multiples_of_master_length: manager && master_manager ? Math.ceil(manager.length / master_manager.length) : 1
    property int current_cycle: manager && master_manager ? Math.floor(manager.pos / master_manager.length) : 0
    
    property alias containsDrag: droparea.containsDrag

    signal selected() //directly selected by the user to be activated.
    signal toggle_in_current_scene() //selected by the user to be added/removed to/from the current scene.
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

            if (widget.containsDrag) {
                return 'yellow';
            } else if (widget.is_in_hovered_scene) {
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

        Item {
            anchors.fill: parent
            anchors.margins: 2

            LoopProgressRect {
                anchors.fill: parent
                manager: statusrect.manager
            }
        }

        ProgressBar {
            id: peak_meter_l
            anchors {
                left: parent.left
                right: parent.horizontalCenter
                bottom: parent.bottom
                margins: 2
            }
            height: 3

            AudioLevelMeterModel {
                id: output_peak_meter_l
                max_dt: 0.1
                input: {
                    if (statusrect.manager && statusrect.manager.wet_looper) {
                        return statusrect.manager.wet_looper.channel_loopers[0].outputPeak
                    } else if (statusrect.manager && statusrect.manager.channel_loopers) {
                        return statusrect.manager.channel_loopers[0].outputPeak
                    } else if (statusrect.manager) {
                        return statusrect.manager.outputPeak
                    }
                    return 0.0
                }
            }

            from: -30.0
            to: 0.0
            value: output_peak_meter_l.value

            background: Item { anchors.fill: peak_meter_l }
            contentItem: Item {
                implicitWidth: peak_meter_l.width
                implicitHeight: peak_meter_l.height

                Rectangle {
                    width: peak_meter_l.visualPosition * peak_meter_l.width
                    height: peak_meter_l.height
                    color: Material.accent
                    x: peak_meter_l.width - width
                }
            }
        }

        ProgressBar {
            id: peak_meter_r
            anchors {
                left: parent.horizontalCenter
                right: parent.right
                bottom: parent.bottom
                margins: 2
            }
            height: 3

            AudioLevelMeterModel {
                id: output_peak_meter_r
                max_dt: 0.1
                input: {
                    if (statusrect.manager && statusrect.manager.wet_looper) {
                        return statusrect.manager.wet_looper.channel_loopers[0].outputPeak
                    } else if (statusrect.manager && statusrect.manager.channel_loopers) {
                        return statusrect.manager.channel_loopers[0].outputPeak
                    } else if (statusrect.manager) {
                        return statusrect.manager.outputPeak
                    }
                    return 0.0
                }
            }

            from: -30.0
            to: 0.0
            value: output_peak_meter_r.value

            background: Item { anchors.fill: peak_meter_r }
            contentItem: Item {
                implicitWidth: peak_meter_r.width
                implicitHeight: peak_meter_r.height

                Rectangle {
                    width: peak_meter_r.visualPosition * peak_meter_r.width
                    height: peak_meter_r.height
                    color: Material.accent
                }
            }
        }

        MouseArea {
            id: area
            x: 0
            y: 0
            anchors.fill: parent
            hoverEnabled: true
            propagateComposedEvents: true
            onPositionChanged: (mouse) => { statusrect.propagateMousePosition(mapToGlobal(mouse.x, mouse.y)) }
            onExited: statusrect.propagateMouseExited()
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

                property bool show_next_state:
                    statusrect.manager && statusrect.manager.state != statusrect.manager.next_state &&
                        statusrect.manager.next_state_countdown >= 0

                LoopStateIcon {
                    id: loopstateicon
                    state: statusrect.manager ? statusrect.manager.state : StatesAndActions.LoopState.Unknown
                    show_timer_instead: parent.show_next_state
                    visible: !parent.show_next_state || (parent.show_next_state && statusrect.manager.next_state_countdown == 0)
                    connected: true
                    size: iconitem.height
                    y: 0
                    anchors.horizontalCenter: iconitem.horizontalCenter
                    onClicked: (event) => {
                            if (event.button === Qt.LeftButton) { contextmenu.popup() }
                            else if (event.button === Qt.MiddleButton) { widget.toggle_in_current_scene() }
                            else if (event.button === Qt.RightButton) { contextmenu.popup() }
                        }
                }
                LoopStateIcon {
                    id: loopnextstateicon
                    state: parent.show_next_state ?
                        statusrect.manager.next_state : StatesAndActions.LoopState.Unknown
                    show_timer_instead: false
                    connected: true
                    size: iconitem.height * 0.65
                    y: 0
                    anchors.right : loopstateicon.right
                    anchors.bottom : loopstateicon.bottom
                    visible: parent.show_next_state
                }
                Text {
                    text: statusrect.manager ? (statusrect.manager.next_state_countdown + 1).toString(): ''
                    visible: parent.show_next_state && statusrect.manager.next_state_countdown > 0
                    anchors.fill: loopstateicon
                    color: Material.foreground
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                    font.bold: true
                }
            }

            Text {
                text: statusrect.name
                color: /\([0-9]+, [0-9]+\)/.test(statusrect.name) ? 'grey' : Material.foreground
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
                visible: statusrect.hovered || playlivefx.hovered || playsolointrack.hovered || recordN.hovered || recordfx.hovered || recordn_menu.visible
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

                    onClicked: { if(statusrect.manager) { statusrect.manager.doLoopAction(StatesAndActions.LoopActionType.DoPlay, [0.0], true) }}

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
                            height: playsolointrack.height + playlivefx.height
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
                                    playsolointrack.onMousePosition(p)
                                    playlivefx.onMousePosition(p)
                                }
                                onExited: { playlivefx.onMouseExited(); playsolointrack.onMouseExited() }
                            }

                            Column {
                                SmallButtonWithCustomHover {
                                    id : playsolointrack
                                    width: buttongrid.button_width
                                    height: buttongrid.button_height
                                    IconWithText {
                                        size: parent.width
                                        anchors.centerIn: parent
                                        name: 'play'
                                        color: 'green'
                                        text_color: Material.foreground
                                        text: "S"
                                    }
                                    onClicked: { if(statusrect.manager) { 
                                        statusrect.manager.doLoopAction(StatesAndActions.LoopActionType.DoPlaySoloInTrack, [0.0], true)
                                        }}

                                    ToolTip.delay: 1000
                                    ToolTip.timeout: 5000
                                    ToolTip.visible: hovered
                                    ToolTip.text: "Play wet recording solo in track."
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
                                    onClicked: { if(statusrect.manager) { statusrect.manager.doLoopAction(StatesAndActions.LoopActionType.DoPlayLiveFX, [0.0], true) }}

                                    ToolTip.delay: 1000
                                    ToolTip.timeout: 5000
                                    ToolTip.visible: hovered
                                    ToolTip.text: "Play dry recording through live effects. Allows hearing FX changes on-the-fly."
                                }
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

                    onClicked: { if(statusrect.manager) { statusrect.manager.doLoopAction(StatesAndActions.LoopActionType.DoRecord, [0.0], true) }}
                    onPressAndHold: { 
                        console.log("hi" )
                        var droppy_component = Qt.createComponent("DraggableRecordIcon.qml")
                        var droppy = droppy_component.createObject(appWindow, {x: 300, y:300, size: 100})
                    }

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

                                    function execute(n_cycles) {
                                        if (statusrect && statusrect.manager) {
                                            statusrect.manager.doLoopAction(StatesAndActions.LoopActionType.DoRecordNCycles, [0.0, n_cycles], true)
                                        }
                                    }

                                    onClicked: execute(n)
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
                                            text: "1 cycle"
                                            onClicked: () => { recordN.execute(1) }
                                        }
                                        MenuItem {
                                            text: "2 cycles"
                                            onClicked: () => { recordN.execute(2) }
                                        }
                                        MenuItem {
                                            text: "3 cycles"
                                            onClicked: () => { recordN.execute(3) }
                                        }
                                        MenuItem {
                                            text: "4 cycles"
                                            onClicked: () => { recordN.execute(4) }
                                        }
                                        MenuItem {
                                            text: "6 cycles"
                                            onClicked: () => { recordN.execute(6) }
                                        }
                                        MenuItem {
                                            text: "8 cycles"
                                            onClicked: () => { recordN.execute(8) }
                                        }
                                        MenuItem {
                                            text: "16 cycles"
                                            onClicked: () => { recordN.execute(16) }
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
                                    onClicked: { if(statusrect.manager) {
                                        var n = widget.n_multiples_of_master_length
                                        var delay = widget.n_multiples_of_master_length - widget.current_cycle - 1
                                        statusrect.manager.doLoopAction(StatesAndActions.LoopActionType.DoReRecordFX,
                                            [delay, n],
                                            true)
                                    }}

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

                    onClicked: { if(statusrect.manager) { statusrect.manager.doLoopAction(StatesAndActions.LoopActionType.DoStop, [0.0], true) }}

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
                    id: volume_dial
                    anchors.fill: parent
                    from: -30.0
                    to:   20.0
                    value: 0.0

                    LinearDbConversion {
                        dB_threshold: parent.from
                        id: convert_volume
                    }

                    onMoved: {
                        convert_volume.dB = volume_dial.value
                        statusrect.manager.volume = convert_volume.linear
                    }

                    Connections {
                        target: statusrect.manager
                        function onVolumeChanged() {
                            convert_volume.linear = statusrect.manager.volume
                            volume_dial.value = convert_volume.dB
                        }
                    }

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
                    return '#004400';
                case StatesAndActions.LoopState.PlayingLiveFX:
                    return '#333300';
                case StatesAndActions.LoopState.Recording:
                    return '#660000';
                case StatesAndActions.LoopState.RecordingFX:
                    return '#663300';
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

        signal clicked(var mouse)

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
                if(lsicon.show_timer_instead) {
                    return Material.foreground
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
            acceptedButtons: Qt.RightButton | Qt.MiddleButton | Qt.LeftButton
            onClicked: (mouse) => lsicon.clicked(mouse)
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
            parent: Overlay.overlay
            x: (parent.width-width) / 2
            y: (parent.height-height) / 2

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
                               if (widget.manager) { widget.manager.doLoopAction(StatesAndActions.LoopActionType.DoClear, [], false) }
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

    // Drop area for responding to drag 'n  drop
    DropArea {
        id: droparea
        anchors.fill: parent
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

        width: 500
        height: 400
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

            Item {
                anchors.left: parent.left
                anchors.right: parent.right
                height: 100
                id: item

                LoopContentWidget {
                    id: waveform
                    waveform_data_max: 1.0
                    min_db: -50.0
                    manager: window.manager
                    samples_per_pixel: manager.length / width
                    anchors.fill: parent

                    Connections {
                        target: window
                        function onVisibleChanged (visible) {
                            if (visible) { waveform.update_data() }
                        }
                    }

                    Connections {
                        target: manager
                        
                        function maybe_update() {
                            if (window.visible &&
                                manager.state != StatesAndActions.LoopState.Recording &&
                                manager.state != StatesAndActions.LoopState.RecordingFX) {
                                    waveform.update_data()
                                }
                        }
                        function onStateChanged() {
                            maybe_update()
                            waveform.recording = manager.state == StatesAndActions.LoopState.Recording ||
                                                 manager.state == StatesAndActions.LoopState.RecordingFX
                        }
                        function onLengthChanged() { maybe_update() }
                        function onLoadedData() { maybe_update() }
                    }
                }
            }
        }
    }
}
