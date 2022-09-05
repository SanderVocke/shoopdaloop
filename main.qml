import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

import 'lib/qml'
import 'lib/LoopState.js' as LoopState

import SLLooperState 1.0
import SLLooperManager 1.0

ApplicationWindow {
    visible: true
    width: 1200
    height: 700
    title: "ShoopDaLoop"

    Material.theme: Material.Dark

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
        property bool is_selected
        property var manager: looper_mgr
        property var state_mgr: looper_state
        signal selected()

        id : widget

        width: childrenRect.width
        height: childrenRect.height

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
            border.color: Material.foreground
            border.width: 1

            MouseArea {
                x: 0
                y: 0
                width: loop.width + parent.x_spacing
                height: loop.height + parent.y_spacing
                onClicked: widget.selected()
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
                        state: looper_state.state
                        connected: looper_state.connected
                        size: 24
                        y: (loop.height - height)/2
                    }
                    TextField {
                        width: 60
                        height: 40
                        font.pixelSize: 15
                        y: (loop.height - height)/2
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

        width: childrenRect.width
        height: childrenRect.height

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

                            onSelected: track.selected_loop = index
                        }
                    }

                    TrackControlWidget {
                        id: trackctlwidget
                        paused: loops.itemAt(track.selected_loop).state_mgr.state === LoopState.LoopState.Paused
                        muted: loops.itemAt(track.selected_loop).state_mgr.state === LoopState.LoopState.Muted
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
        property int num_tracks
        property int loops_per_track

        width: childrenRect.width
        height: childrenRect.height
        y: 0

        Rectangle {
            property int x_spacing: 16
            property int y_spacing: 10

            width: childrenRect.width + x_spacing
            height: childrenRect.height + y_spacing
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
                        }
                    }
                }
            }
        }
    }

    component SceneWidget : Rectangle {
        property bool selected
        property string name

        color: selected ? Material.foreground : Material.background
        border.color: Material.foreground
        border.width: 1
        radius: 5

        Text {
            text: name
            color: selected ? Material.background : Material.foreground
            anchors.centerIn: parent
        }
    }

    component ScenesWidget: Item {
        id: sceneswidget

        width: 100
        height: 100

        property int selected: 0
        property var items: [
            { name: "Awesome" },
            { name: "Bawesome" }
        ]

        Rectangle {
            width: parent.width
            height: parent.height
            property int x_spacing: 8
            property int y_spacing: 4

            color: "#555555"

            Item {
                width: parent.width - parent.x_spacing
                height: parent.height - parent.y_spacing
                x: parent.x_spacing/2
                y: parent.y_spacing/2

                Rectangle {
                    width: parent.width
                    height: 300
                    border.color: Material.foreground
                    border.width: 3
                    color: 'transparent'

                    ScrollView {
                        anchors.fill: parent
                        ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
                        ScrollBar.vertical.policy: ScrollBar.AsNeeded

                        Column {
                            spacing: 1
                            anchors.fill: parent

                            Repeater {
                                model: sceneswidget.items.length

                                SceneWidget {
                                    width: parent.width
                                    height: 20
                                    name: sceneswidget.items[index].name
                                    selected: index === sceneswidget.selected
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Column {
        Image {
            x: 40
            height: 60
            width: height / sourceSize.height * sourceSize.width
            source: 'resources/shoopdawhoop.png'
            smooth: true
        }

        Row {
            spacing: 5

            TracksWidget {
                num_tracks: 8
                loops_per_track: 8
            }
            ScenesWidget {
                width: 200
                height: 400
            }
        }
    }
}
