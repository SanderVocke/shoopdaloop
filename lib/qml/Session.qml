import QtQuick 2.15

Item {
    id: session

    // The descriptor is an object matching the ShoopDaLoop session JSON
    // schema. The Session object will manage an actual session (consisting)
    // of loops, tracks, etc.) to match the loaded descriptor.
    // Changes in the session are fed back into the descriptor.
    // The descriptor may not be directly modified.
    property var descriptor : ({
        'schema': 'session.1',
        'tracks': [],
        'ports': []
    })

    SchemaCheck {
        descriptor: session.descriptor
        schema: 'session.1'
    }

    // Objects registry just stores object ids -> objects.
    property Registry objects_registry: Registry { verbose: true }

    // State registry stores the following optional states:
    // - "master_loop" -> LoopWidget which holds the master loop
    // - "targeted_loop" -> LoopWidget which is currently targeted
    property Registry state_registry: Registry { verbose: true }

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
            
            // Master loop track item.
            TrackWidget {
                id: masterlooptrack
                anchors {
                    top: parent.top
                    //right: other_tracks.left
                    //rightMargin: 6
                }
                descriptor: session.descriptor.tracks[0]
                objects_registry: session.objects_registry
                state_registry: session.state_registry
            }

            Repeater {
                model: session.descriptor.tracks.length - 1

                TrackWidget {
                    anchors {
                        top: parent.top
                        //right: other_tracks.left
                        //rightMargin: 6
                    }
                    descriptor: session.descriptor.tracks[index+1]
                    objects_registry: session.objects_registry
                    state_registry: session.state_registry
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