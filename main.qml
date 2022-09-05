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
        property string state
        name: 'help-circle'
        color: 'red'

        function getName() {
            switch(state) {
            case LoopState.LoopState.Playing:
                return 'play'
            case LoopState.LoopState.Recording:
                return 'record'
            default:
                return 'help-circle'
            }
        }

        function getColor() {
            switch(state) {
            case LoopState.LoopState.Playing:
                return 'green'
            case LoopState.LoopState.Recording:
                return 'red'
            default:
                return 'red'
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
        Connections {
            target: osc_link_obj
            function onReceived(msg) { looper_mgr.onOscReceived(msg) }
        }
        Connections {
            target: looper_mgr
            function onLengthChanged(len) { looper_state.length = len }
            function onPosChanged(pos) { looper_state.pos = pos }
            function onActiveChanged(active) { looper_state.active = active }
            function onSendOscExpectResponse(msg, ret) { osc_link_obj.send_expect_response(msg, ret) }
            function onSendOsc(msg) { osc_link_obj.send(msg) }
        }

        // Initialization
        Component.onCompleted: {
            looper_mgr.setup_sync()
        }

        // UI
        //LoopProgressIndicator {
        //    id: progress
        //    length: looper_state.length
        //    pos: looper_state.pos
        //    active: looper_state.active
        //}
        Rectangle {
            property int x_spacing: 16
            property int y_spacing: 10

            width: childrenRect.width + x_spacing
            height: childrenRect.height + y_spacing
            color: Material.background
            border.color: Material.foreground
            border.width: 2

            Item {
                width: childrenRect.width
                height: childrenRect.height
                x: parent.x_spacing/2
                y: parent.y_spacing/2

                Row {
                    LoopStateIcon {
                        size: 32
                    }
                    TextField {
                        width: 80
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
                    }

                    Repeater {
                        model: track.num_loops
                        id: loops

                        LoopWidget {
                            loop_idx: track.first_index + index
                            osc_link_obj: osc_link
                        }
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

    TracksWidget {
        num_tracks: 8
        loops_per_track: 8
    }
}
