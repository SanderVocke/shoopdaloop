import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3

Item {
    id: root
    
    property var loop // LoopWidget
    property var composite_loop : loop.maybe_composite_loop

    property int cycle_width: 90
    readonly property int cycle_length: composite_loop.sync_length
    property int swimlane_height: 26

    property var scheduled_playlists : composite_loop.scheduled_playlists
    property int schedule_length : Math.max.apply(null, Object.keys(composite_loop.schedule))

    property var sync_track
    property var main_tracks : []

    Column {
        id: tracks_mapper
        spacing: 1
        width: parent.width
        height: childrenRect.height

        Mapper {
            model : main_tracks

            Track {
                property var mapped_item
                property int index

                track : mapped_item
                width: tracks_mapper.width
            }
        }
    }

    // Make a representation of the playlists in which element are Qt objects.
    // That way they can reference and update each other.
    component PlaylistElement : QtObject {
        // set based on scheduling
        property var loop
        property string loop_id
        property int start_iteration
        property int end_iteration
        property var incoming_edge // refer to other PlaylistElement
        property var outgoing_edge // refer to other PlaylistElement

        // set when a GUI item representing this element is created
        property var gui_item
    }

    readonly property var playlist_elems : {
        // TODO instantiate the elements
    }

    component Track : Item {
        id: track_root

        property var track
        readonly property int track_idx : track.track_idx
        
        height: childrenRect.height

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
        readonly property int track_schedule_length : Math.max.apply(null, Object.keys(track_schedule))

        // Get a list of loop triggers from the schedule.
        // Result has form:
        // [
        //    { 'start': starting_cycle, 'end': ending_cycle, 'loop': LoopWidget }   
        // ]
        readonly property var loop_triggers : {
            var rval = []
            var track_active_loop_starts = []

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
            return rval
        }

        // Split the trigger list into "swimlanes" in order
        // to handle with multiple loops in the same slot.
        // Result is equivalent to "loop_triggers", except
        // an integer "swimlane" is added to each entry.
        readonly property var loop_triggers_swimlanes : {
            let create_slot_status = () => new Array(track_schedule_length).fill(false)
            var rval = loop_triggers

            // keep the filled status of each cycle per swimlane.
            var slots_taken = [create_slot_status()]

            for(var i=0; i<rval.length; i++) {
                const item = rval[i]
                // fn to check if a suggested slot is free
                let is_free = (swimlane_idx) => {
                    for(var j=item['start']; j<item['end']; j++) {
                        if (slots_taken[swimlane_idx][j]) { return false; }
                    }
                    return true;
                }

                // find a free slot or create a new swimlane.
                // first, check existing swimlanes for occurences of the same
                // loop which are already scheduled. If any, try that swimlane first
                // so that loops are grouped nicely together.
                var same_loops = rval.filter(v => v.swimlane != undefined && v.loop == item.loop)
                var occurrences_per_lane = slots_taken.map((v, idx) => same_loops.filter(v => v.swimlane == idx).length)
                var max = -1
                var argmax = -1
                for (var k=0; k<occurrences_per_lane.length; k++) {
                    if (occurrences_per_lane[k] > max) {
                        argmax = k
                        max = occurrences_per_lane[k]
                    }
                }
                if (argmax >= 0 && max > 0 && is_free(argmax)) {
                    item['swimlane'] = argmax
                } else {
                    for(var k=0; k<slots_taken.length; k++) {
                        if (is_free(k)) {
                            item['swimlane'] = k;
                            break;
                        }
                    }
                }
                if (!item.hasOwnProperty('swimlane')) {
                    // new swimlane needed
                    slots_taken.push(create_slot_status())
                    item['swimlane'] = slots_taken.length - 1
                }

                // Mark the newly taken slots
                for(var h=item['start']; h<item['end']; h++) {
                    slots_taken[item.swimlane][h] = true
                }
            }
            return rval
        }

        readonly property int n_swimlanes: Math.max.apply(null, loop_triggers_swimlanes.map(l => l['swimlane'])) + 1

        Label {
            id: name_label
            anchors {
                left: parent.left
                top: content_rect.top
                bottom: content_rect.bottom
            }
            text: track.name
            horizontalAlignment: Text.AlignRight
            verticalAlignment: Text.AlignVCenter
            width: 80
        }

        Rectangle {
            id: content_rect
            anchors {
                top: parent.top
                left: name_label.right
                right: parent.right
            }
            height: Math.max(childrenRect.height, root.swimlane_height)
            color: Material.background

            Column {
                id: swimlanes_column
                spacing: 1
                
                height: childrenRect.height
                width: root.cycle_width * track_root.track_schedule_length
                Repeater {
                    id: swimlanes
                    model: track_root.n_swimlanes
                    anchors {
                        left: parent.left
                        right: parent.right
                        top: parent.top
                    }

                    Rectangle {
                        id: swimlane
                        border.color: 'black'
                        border.width: 1
                        color: 'transparent'

                        width: swimlanes.width
                        height: root.swimlane_height

                        Mapper {
                            model : track_root.loop_triggers_swimlanes.filter(l => l.swimlane == index)

                            Rectangle {
                                property var mapped_item
                                property int index

                                color: 'grey'
                                border.color: 'black'
                                border.width: 1

                                width: root.cycle_width * (mapped_item.end - mapped_item.start)
                                height: swimlane.height
                                x: root.cycle_width * mapped_item.start

                                Label {
                                    anchors.centerIn: parent
                                    text: mapped_item.loop.name
                                }
                            }
                        }
                    }
                }
            }

            Rectangle {
                id: play_head
                width: 3
                anchors {
                    top: swimlanes_column.top
                    bottom: swimlanes_column.bottom
                }
                color: 'green'
                visible: parseInt(x) != 0

                x: (composite_loop.position * root.cycle_width) / root.cycle_length
            }
        }
    }
}