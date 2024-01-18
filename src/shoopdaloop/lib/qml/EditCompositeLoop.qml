import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3

Item {
    id: root
    
    property var loop // LoopWidget
    property var composite_loop : loop.maybe_composite_loop

    property int cycle_width: 90
    property int track_height: 26

    property var schedule: composite_loop.schedule
    property int schedule_length : Math.max.apply(null, Object.keys(schedule))

    property var sync_track
    property var main_tracks : []

    Column {
        spacing: 1

        Mapper {
            model : [sync_track].concat(main_tracks)

            Track {
                property var mapped_item
                property int index

                track : mapped_item

                height: root.track_height
                width: root.width
            }
        }
    }

    component Track : Item {
        id: track_root

        property var track
        readonly property int track_idx : track.track_idx

        // Reduce the schedule to only the events for this track
        readonly property var track_schedule : {
            if (!root.schedule) { return {}; }
            var rval = {}
            for(var k of Array.from(Object.keys(root.schedule))) { //iteration
                rval[k] = {}
                for(var kk of Array.from(Object.keys(root.schedule[k]))) { //loops start / loops end for this iteration
                    rval[k][kk] = root.schedule[k][kk].filter(function (x) { return x.loop_widget.track_idx == track_idx; })
                }
            }
            return rval
        }

        // Get a list of loop triggers from the schedule
        readonly property var loop_triggers : {
            var rval = []
            var track_active_loop_starts = []

            if (track_idx == -1) {
                // For sync track, just show repeats of the sync loop running each slot.
                for(var i = 0; i < root.schedule_length; i++) { //iteration
                    rval.push({
                        'start': i,
                        'end': i+1,
                        'loop': root.sync_track.loops[0]
                    })
                }
            } else {
                for(var k of Array.from(Object.keys(track_schedule))) { //iteration
                    for(var loop of track_schedule[k].loops_start) {
                        track_active_loop_starts.push({
                            'start': parseInt(k),
                            'loop': loop
                        })
                    }
                    for(var loop of track_schedule[k].loops_end) {
                        for(var i = 0; i < track_active_loop_starts.length; i++) {
                            if (track_active_loop_starts[i].loop == loop) {
                                let loop_cycles = loop.loop_widget.n_cycles
                                let start = track_active_loop_starts[i].start
                                let end = parseInt(k)
                                for (var j=0; j<(end - start)/loop_cycles; j++) {
                                    rval.push({
                                        'start': start+j*loop_cycles,
                                        'end': Math.min(start+(j+1)*loop_cycles, end),
                                        'loop': loop.loop_widget
                                    })
                                }
                                track_active_loop_starts.splice(i, 1)
                                break
                            }
                        }
                    }
                }
            }
            return rval
        }

        Label {
            id: name_label
            anchors {
                left: parent.left
                verticalCenter: parent.verticalCenter
            }
            text: track.name
            horizontalAlignment: Text.AlignRight
            width: 80
        }

        Rectangle {
            anchors {
                top: parent.top
                bottom: parent.bottom
                left: name_label.right
            }

            border.color: 'black'
            border.width: 1
            color: 'transparent'

            width: root.cycle_width * root.schedule_length

            Mapper {
                model : track_root.loop_triggers

                Rectangle {
                    property var mapped_item
                    property int index

                    color: 'grey'
                    border.color: 'black'
                    border.width: 1

                    width: root.cycle_width * (mapped_item.end - mapped_item.start)
                    x: root.cycle_width * mapped_item.start

                    anchors {
                        top: parent.top
                        bottom: parent.bottom
                    }

                    Label {
                        anchors.centerIn: parent
                        text: mapped_item.loop.name
                    }
                }
            }
        }
    }
}