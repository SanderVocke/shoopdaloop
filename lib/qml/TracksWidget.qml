import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

// The tracks widget displays the state of a number of tracks.
Item {
    id: tracks
    width: childrenRect.width
    height: childrenRect.height

    property var track_names: []
    property int loops_per_track
    property var master_loop_idx: [0, 0] // track index, loop index in track TODO move to shared state
    property var maybe_master_loop_manager // to be set from children

    //Arrays of [track, loop]
    property var loops_of_selected_scene: []
    property var loops_of_hovered_scene: []

    signal request_bind_loop_to_scene(int track, int loop)
    signal request_rename(int track, string name)

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
                    model: tracks.track_names ? tracks.track_names.length : 0
                    id: tracks_repeater

                    TrackWidget {
                        name: tracks.track_names[index]
                        num_loops: tracks.loops_per_track
                        first_index: index * tracks.loops_per_track
                        track_index: index + 1
                        maybe_master_loop_idx: tracks.master_loop_idx[0] === index ? tracks.master_loop_idx[1] : -1
                        master_loop_manager: tracks.maybe_master_loop_manager

                        function unpack(loops) {
                            var r = []
                            var l
                            for (l in loops) { if (loops[l][0] === index) { r.push(loops[l][1]) } }
                            return r
                        }

                        loops_of_selected_scene: unpack(tracks.loops_of_selected_scene)
                        loops_of_hovered_scene: unpack(tracks.loops_of_hovered_scene)

                        onSet_loop_in_scene: (l) => tracks.request_bind_loop_to_scene(index, l)
                        onRenamed: (name) => tracks.request_rename(index, name)
                        onMaster_loop_created: (m) => tracks.maybe_master_loop_manager = m
                    }
                }
            }
        }
    }
}
