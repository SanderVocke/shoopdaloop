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
    height: 800
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
            property int x_spacing: 16
            property int y_spacing: 10

            width: loop.width + x_spacing
            height: loop.height + y_spacing
            color: widget.is_selected ? Material.accent : Material.background
            border.color: Material.foreground
            border.width: 2

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
                        placeholderText: "loop"
                        width: 60
                        y: (loop.height - height)/2
                    }
                }
            }
        }
    }

    // The track control widget displays control buttons to control the
    // (loops within a) track.
    component TrackControlWidget : Item {
        width: childrenRect.width
        height: childrenRect.height

        Slider {
            orientation: Qt.Vertical
            height: 120
            from: 0.0
            to: 1.0
            value: 1.0
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
            property int x_spacing: 16
            property int y_spacing: 10

            width: childrenRect.width + x_spacing
            height: childrenRect.height + y_spacing
            color: "#555555"

            Item {
                width: childrenRect.width
                height: childrenRect.height
                x: parent.x_spacing/2
                y: parent.y_spacing/2

                Column {
                    spacing: 5

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
                            is_selected: track.selected_loop == index

                            onSelected: track.selected_loop = index
                        }
                    }

                    TrackControlWidget {

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
                    spacing: 5

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

    Column {
        Image {
            x: 40
            height: 100
            width: height / sourceSize.height * sourceSize.width
            source: 'resources/shoopdawhoop.png'
            smooth: true
        }

        TracksWidget {
            num_tracks: 8
            loops_per_track: 8
        }
    }
}
