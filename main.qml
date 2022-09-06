import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

import 'lib/qml'
import 'lib/LoopState.js' as LoopState

import SLLooperState 1.0
import SLLooperManager 1.0

ApplicationWindow {
    visible: true
    width: 1050
    height: 600
    maximumWidth: width
    maximumHeight: height
    minimumWidth: width
    minimumHeight: height
    title: "ShoopDaLoop"

    Material.theme: Material.Dark

    // Ensure other controls lose focus when clicked outside
    MouseArea {
        id: background_focus
        anchors.fill: parent
        acceptedButtons: Qt.AllButtons
        onClicked: () => { forceActiveFocus() }
    }

    // The loop progress indicator displays the position of the loop
    // within its length.
    component LoopProgressIndicator : Item {
        property alias length: pb.length
        property alias pos: pb.pos
        property bool active

        width: childrenRect.width
        height: childrenRect.height

        ProgressBar {
            width: 80
            height: 25
            id: pb
            property double length
            property double pos

            value: active ? pos / length : 0.0
        }
        Text {
            text: active ? pb.pos.toFixed(2) : "(inactive)"
            color: Material.foreground
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

    // The loop widget displays the state of a single loop within a track.
    component LoopWidget : Item {
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
    }

    // The track control widget displays control buttons to control the
    // (loops within a) track.
    component TrackControlWidget : Item {
        id: trackctl
        width: childrenRect.width
        height: childrenRect.height

        signal pause()
        signal unpause()
        signal mute()
        signal unmute()
        signal record()

        property bool paused
        property bool muted

        Column {
            spacing: 0
            width: 100

            Slider {
                id: volume
                x: 0
                y: 0

                orientation: Qt.Horizontal
                width: parent.width
                height: 20
                from: 0.0
                to: 1.0
                value: 1.0
            }
            Slider {
                id: pan
                x: 0
                y: 0

                orientation: Qt.Horizontal
                width: parent.width
                height: 20
                from: -1.0
                to: 1.0
                value: 0.0
            }

            Column {
                width: childrenRect.width
                height: childrenRect.height
                x: (parent.width - width) / 2

                spacing: 2

                Row {
                    spacing: 2

                    Button {
                        id : record
                        width: 30
                        height: 30
                        MaterialDesignIcon {
                            size: parent.width - 10
                            anchors.centerIn: parent
                            name: 'record'
                            color: 'red'
                        }
                        onClicked: { trackctl.record() }
                    }
                    Button {
                        id : pause
                        width: 30
                        height: 30
                        MaterialDesignIcon {
                            size: parent.width - 10
                            anchors.centerIn: parent
                            name: trackctl.paused ? 'play' : 'pause'
                            color: Material.foreground
                        }
                        onClicked: { if(pause.paused) {trackctl.unpause()} else {trackctl.pause()} }
                    }
                    Button {
                        id : mute
                        width: 30
                        height: 30
                        MaterialDesignIcon {
                            size: parent.width - 10
                            anchors.centerIn: parent
                            name: trackctl.muted ? 'volume-mute' : 'volume-high'
                            color: trackctl.muted ? 'grey' : Material.foreground
                        }
                        onClicked: { if(mute.muted) {trackctl.unmute()} else {trackctl.mute()} }
                    }
                }

                Rectangle {
                    width: childrenRect.width
                    height: childrenRect.height
                    color: 'grey'

                    Column {
                        width: childrenRect.width
                        height: childrenRect.height
                        spacing: 0

                        Text {
                            text: "fx proc"
                            color: Material.foreground
                            width: 50
                            topPadding: 2
                            leftPadding: 5
                            bottomPadding: 0
                            font.pixelSize: 15
                        }

                        Row {
                            width: childrenRect.width
                            height: childrenRect.height
                            spacing: 2

                            Button {
                                id : fx_off
                                width: 30
                                height: 30
                                MaterialDesignIcon {
                                    size: parent.width - 10
                                    anchors.centerIn: parent
                                    name: 'close'
                                    color: Material.foreground
                                }
                            }
                            Button {
                                id : fx_live
                                width: 30
                                height: 30
                                MaterialDesignIcon {
                                    size: parent.width - 10
                                    anchors.centerIn: parent
                                    name: 'record'
                                    color: Material.foreground
                                }
                            }
                            Button {
                                id : fx_cached
                                width: 30
                                height: 30
                                MaterialDesignIcon {
                                    size: parent.width - 10
                                    anchors.centerIn: parent
                                    name: 'sd'
                                    color: Material.foreground
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // The track widget displays the state of a track (collection of
    // loopers with shared settings/control).
    component TrackWidget : Item {
        id: track
        property int num_loops
        property int first_index
        property int track_index
        property int selected_loop
        property int maybe_master_loop: -1 //-1 is none

        // Array of loop idxs
        property var loops_of_selected_scene: []
        property var loops_of_hovered_scene: []

        width: childrenRect.width
        height: childrenRect.height

        signal set_loop_in_scene(int idx)

        Rectangle {
            property int x_spacing: 8
            property int y_spacing: 4

            width: childrenRect.width + x_spacing
            height: childrenRect.height + y_spacing
            color: "#555555"

            Item {
                width: childrenRect.width
                height: childrenRect.height
                x: parent.x_spacing/2
                y: parent.y_spacing/2

                Column {
                    spacing: 2

                    TextField {
                        placeholderText: qsTr("Track " + track.track_index.toString())
                        width: 90
                        font.pixelSize: 15

                        onEditingFinished: background_focus.forceActiveFocus()
                    }

                    Repeater {
                        model: track.num_loops
                        id: loops
                        width: childrenRect.width
                        height: childrenRect.height

                        LoopWidget {
                            loop_idx: track.first_index + index
                            osc_link_obj: osc_link
                            is_selected: track.selected_loop === index
                            is_master: track.maybe_master_loop === index
                            is_in_selected_scene: track.loops_of_selected_scene.includes(index)
                            is_in_hovered_scene: track.loops_of_hovered_scene.includes(index)

                            onSelected: track.selected_loop = index
                            onAdd_to_scene: () => { track.set_loop_in_scene(index) }
                        }
                    }

                    TrackControlWidget {
                        id: trackctlwidget

                        function active_loop_state_equals(s) {
                            if (loops.length > 0) {
                                return loops.itemAt(track.selected_loop).state_mgr.state === s
                            }
                            return false
                        }

                        paused: active_loop_state_equals(LoopState.LoopState.Paused)
                        muted: active_loop_state_equals(LoopState.LoopState.Muted)
                    }

                    Connections {
                        target: trackctlwidget
                        function onRecord() { loops.itemAt(track.selected_loop).manager.doRecord() }
                        function onPause() { loops.itemAt(track.selected_loop).manager.doPlayPause() }
                        function onUnpause() { loops.itemAt(track.selected_loop).manager.doPlayPause() }
                        function onMute() { loops.itemAt(track.selected_loop).manager.doMute() }
                        function doUnmute() { loops.itemAt(track.selected_loop).manager.doUnmute() }
                    }
                }
            }
        }
    }

    // The tracks widget displays the state of a number of tracks.
    component TracksWidget : Item {
        id: tracks
        width: childrenRect.width
        height: childrenRect.height

        property int num_tracks
        property int loops_per_track
        property var master_loop: [0, 0] // track index, loop index in track

        //Arrays of [track, loop]
        property var loops_of_selected_scene: []
        property var loops_of_hovered_scene: []

        signal set_loop_in_scene(int track, int loop)

        Rectangle {
            property int x_spacing: 0
            property int y_spacing: 0

            width: childrenRect.width + x_spacing
            height: childrenRect.height + y_spacing
            anchors.top: parent.top
            color: Material.background

            Item {
                width: childrenRect.width
                height: childrenRect.height
                x: parent.x_spacing/2
                y: parent.y_spacing/2

                Row {
                    spacing: 3

                    Repeater {
                        model: tracks.num_tracks
                        id: tracks_repeater

                        TrackWidget {
                            num_loops: tracks.loops_per_track
                            first_index: index * tracks.loops_per_track
                            track_index: index + 1
                            maybe_master_loop: tracks.master_loop[0] === index ? tracks.master_loop[1] : -1

                            function unpack(loops) {
                                var r = []
                                var l
                                for (l in loops) { if (loops[l][0] === index) { r.push(loops[l][1]) } }
                                return r
                            }

                            loops_of_selected_scene: unpack(tracks.loops_of_selected_scene)
                            loops_of_hovered_scene: unpack(tracks.loops_of_hovered_scene)

                            onSet_loop_in_scene: (l) => tracks.set_loop_in_scene(index, l)
                        }
                    }
                }
            }
        }
    }

    component SceneWidget : Rectangle {
        id: scenewidget

        property bool is_selected
        property string name

        //Array of [track, loop]
        property var referenced_loops: []

        signal clicked()
        signal hoverEntered()
        signal hoverExited()
        signal nameEntered(string name)

        color: is_selected ? Material.foreground : Material.background
        border.color: marea.containsMouse ? 'blue' : is_selected ? 'red' : 'grey'
        border.width: 2

        MouseArea {
            id: marea
            hoverEnabled: true
            anchors.fill: parent
            onClicked: scenewidget.clicked()
            onEntered: hoverEntered()
            onExited: hoverExited()
        }

        TextField {
            width: parent.width - 30
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.horizontalCenterOffset: 8

            text: name
            color: is_selected ? Material.background : Material.foreground
            anchors.centerIn: parent

            onEditingFinished: () => { nameEntered(displayText); background_focus.forceActiveFocus() }
        }
    }

    component ScenesWidget: Item {
        id: sceneswidget

        property int selected_scene: -1
        property int hovered_scene: -1
        property var items: [
            // { name: "scene_name", loops: [[0, 0], [1,4]] }
        ]

        function get_referenced_loops_for_selected_scene() {
            if (selected_scene >= 0) {
                return items[selected_scene].loops
            }
            return []
        }
        function get_referenced_loops_for_hovered_scene() {
            if (hovered_scene >= 0) {
                return items[hovered_scene].loops
            }
            return []
        }
        function set_loop_in_current_scene(track, loop) {
            if (selected_scene >= 0) {
                // remove any previous setting for this track
                var modified = items[selected_scene].loops
                modified = modified.filter(l => l[0] !== track)
                // add the new setting
                modified.push([track, loop])
                items[selected_scene].loops = modified
                itemsChanged()
            }
        }
        function add_scene() {
            var name_idx = 1
            var candidate
            while (true) {
                candidate = 'scene ' + name_idx.toString()
                name_idx = name_idx + 1
                var found = false
                var idx
                for (idx in items) { if(items[idx].name == candidate) { found = true; break; } }
                if (!found) { break }
            }

            items.push({ name: candidate, loops: [] })
            itemsChanged()
        }
        function remove_selected_scene() {
            if (selected_scene >= 0) {
                items.splice(selected_scene, 1)
                if (hovered_scene > selected_scene) {
                    hovered_scene = hovered_scene - 1
                    hovered_sceneChanged()
                }
                else if(hovered_scene == selected_scene) {
                    hovered_scene = -1
                    hovered_sceneChanged()
                }
                selected_scene = -1
                selected_sceneChanged()
                itemsChanged()
            }
        }

        // Arrays of [track, loop]
        property var referenced_loops_selected_scene: get_referenced_loops_for_selected_scene()
        property var referenced_loops_hovered_scene: get_referenced_loops_for_hovered_scene()

        Rectangle {
            width: parent.width
            height: parent.height
            anchors.top: parent.top
            property int x_spacing: 8
            property int y_spacing: 4

            color: "#555555"

            Column {
                anchors {
                    fill: parent
                    leftMargin: 4
                    rightMargin: 4
                    topMargin: 2
                    bottomMargin: 2
                }

                Item {
                    height: 40
                    anchors {
                        left: parent.left
                        right: parent.right
                    }

                    Text {
                        text: "Scenes"
                        font.pixelSize: 15
                        color: Material.foreground
                        anchors {
                            centerIn: parent
                        }
                    }
                }

                Rectangle {
                    anchors {
                        left: parent.left
                        right: parent.right
                    }
                    height: 360
                    border.color: 'grey'
                    border.width: 1
                    color: 'transparent'

                    ScrollView {
                        anchors {
                            fill: parent
                            margins: parent.border.width + 1
                        }

                        ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
                        ScrollBar.vertical.policy: ScrollBar.AsNeeded

                        Column {
                            spacing: 1
                            anchors.fill: parent

                            Repeater {
                                model: sceneswidget.items.length
                                anchors.fill: parent

                                SceneWidget {
                                    anchors {
                                        right: parent.right
                                        left: parent.left
                                    }

                                    height: 40
                                    name: sceneswidget.items[index].name
                                    is_selected: index === sceneswidget.selected_scene

                                    Connections {
                                        function onClicked() { sceneswidget.selected_scene = index }
                                        function onHoverEntered() { sceneswidget.hovered_scene = index }
                                        function onHoverExited() { if (sceneswidget.hovered_scene === index) { sceneswidget.hovered_scene = -1; } }
                                        function onNameEntered(name) { sceneswidget.items[index].name = name; sceneswidget.itemsChanged() }
                                    }
                                }
                            }
                        }
                    }
                }

                Row {
                    anchors {
                        left: parent.left
                        right: parent.right
                    }
                    spacing: 4

                    Button {
                        width: 30
                        height: 40
                        MaterialDesignIcon {
                            size: 20
                            name: 'plus'
                            color: Material.foreground
                            anchors.centerIn: parent
                        }
                        onClicked: add_scene()
                    }
                    Button {
                        width: 30
                        height: 40
                        MaterialDesignIcon {
                            size: 20
                            name: 'delete'
                            color: Material.foreground
                            anchors.centerIn: parent
                        }
                        onClicked: remove_selected_scene()
                    }
                }
            }
        }
    }

    component ScriptingWidget : Rectangle {
        color: "#555555"

        Row {
            anchors.fill: parent
            anchors.margins: 6
            Text {
                text: "Scripting"
                font.pixelSize: 15
                color: Material.foreground
                verticalAlignment: Text.AlignTop
                anchors {
                    top: parent.top
                    bottom: parent.bottom
                }
            }
        }
    }

    Item {
        anchors {
            fill: parent
            margins: 6
        }

        Item {
            id: tracks_scenes_row

            anchors {
                left: parent.left
                right: parent.right
            }
            height: tracks_widget.height

            TracksWidget {
                id: tracks_widget
                anchors.top: parent.top

                num_tracks: 8
                loops_per_track: 8
                loops_of_selected_scene: scenes.referenced_loops_selected_scene
                loops_of_hovered_scene: scenes.referenced_loops_hovered_scene

                onSet_loop_in_scene: (track, loop) => { scenes.set_loop_in_current_scene(track, loop) }
            }

            ScenesWidget {
                anchors {
                    top: tracks_widget.top
                    bottom: tracks_widget.bottom
                    left: tracks_widget.right
                    leftMargin: 6
                    right: parent.right
                }

                id: scenes
            }
        }

        Item {
            anchors {
                left: tracks_scenes_row.left
                right: tracks_scenes_row.right
                top: tracks_scenes_row.bottom
                topMargin: 6
                bottom: parent.bottom
            }

            Image {
                id: logo
                anchors {
                    top: parent.top
                    topMargin: 20
                }

                height: 60
                width: height / sourceSize.height * sourceSize.width
                source: 'resources/logo-small.png'
                smooth: true
            }

            ScriptingWidget {
                anchors {
                    top: parent.top
                    bottom: parent.bottom
                    left: logo.right
                    leftMargin: 10
                    right: parent.right
                }
            }
        }
    }
}
