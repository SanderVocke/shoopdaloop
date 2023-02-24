import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

import "../generate_session.js" as GenerateSession

Item {
    id: session

    // The descriptor is an object matching the ShoopDaLoop session JSON
    // schema. The Session object will manage an actual session (consisting)
    // of loops, tracks, etc.) to match the loaded descriptor.
    // The descriptor may not be directly modified and is only used at initialization.
    // To get an up-to-date descriptor of the current state, call generate_descriptor().
    property var initial_descriptor : ({
        'schema': 'session.1',
        'tracks': [],
        'ports': []
    })

    SchemaCheck {
        descriptor: session.initial_descriptor
        schema: 'session.1'
    }

    // Objects registry just stores object ids -> objects.
    property Registry objects_registry: Registry { verbose: false }

    // State registry stores the following optional states:
    // - "master_loop" -> LoopWidget which holds the master loop
    // - "targeted_loop" -> LoopWidget which is currently targeted
    property Registry state_registry: Registry { verbose: false }

    property var track_factory : Qt.createComponent("TrackWidget.qml")
    property alias tracks : tracks_row.children
    function add_track(properties) {
        if (track_factory.status != Component.Ready) {
            throw new Error("Track factory not ready")
        }
        var track = track_factory.createObject(session, properties);
        session.tracks.push(track)
    }

    Component.onCompleted: {
        session.initial_descriptor.tracks.forEach(desc => {
            session.add_track({
                descriptor: desc,
                objects_registry: session.objects_registry,
                state_registry: session.state_registry
            });
        })
    }

    Backend {
        update_interval_ms: 30
        client_name_hint: 'ShoopDaLoop'
        id: backend

        anchors {
            fill: parent
            margins: 6
        }

        Row {
            spacing: 3

            Row {
                spacing: 3
                id: tracks_row
                // Note: tracks injected here
            }

            Column {
                spacing: 3

                Button {
                    width: 30
                    height: 40
                    MaterialDesignIcon {
                        size: 20
                        name: 'plus'
                        color: Material.foreground
                        anchors.centerIn: parent
                    }
                    onClicked: {
                        var idx = (session.tracks.length).toString()
                        var track_descriptor = GenerateSession.generate_default_track("Track " + idx, 1, 'track_' + idx)
                        session.add_track({
                            descriptor: track_descriptor,
                            objects_registry: session.objects_registry,
                            state_registry: session.state_registry
                        })
                    }
                }

                Button {
                    width: 30
                    height: 40
                    MaterialDesignIcon {
                        size: 20
                        name: 'close'
                        color: Material.foreground
                        anchors.centerIn: parent
                    }
                    onClicked: {
                        for(var i=session.tracks.length-1; i>0; i--) {
                            session.tracks[i].qml_close()
                        }
                        session.tracks.length = 1;
                    }
                }
            }
        }

        Item {
            //spacing: 3
            width: 500
            height: 300
        }

        // property LoopWidget master_loop : null
        // property LoopWidget targeted_loop : null
        // property list<LoopWidget> all_loops : []
        // property var loop_coordinates : ({})

        // function set_targeted_loop(loop) {
        //     for(var i = 0; i<all_loops.length; i++) {
        //         if (all_loops[i] == loop) {
        //             all_loops[i].target()
        //         } else {
        //             all_loops[i].untarget()
        //         }
        //     }
        //     targeted_loop = loop
        // }

        // function transition_selected(mode, delay, wait_for_sync, exclude_loop) {
        //     for(var i = 0; i<all_loops.length; i++) {
        //         if (all_loops[i].selected && all_loops[i] != exclude_loop) {
        //             all_loops[i].transition(mode, delay, wait_for_sync, false)
        //         }
        //     }
        // }

        // function register_loop(track_index, loop_index, loop, is_master) {
        //     if (is_master) { master_loop = loop }
        //     all_loops.push(loop)
        //     loop_coordinates[loop] = [track_index, loop_index]
        //     console.log('connecteroo')
        //     loop.onTargetedChanged.connect(() => {
        //         set_targeted_loop(loop)
        //     })
        //     loop.onTransition.connect((mode, delay, wait_for_sync) => {
        //         transition_selected(mode, delay, wait_for_sync, loop)
        //     })
        // }
    }
}