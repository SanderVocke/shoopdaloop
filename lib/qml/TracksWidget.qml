import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

// The tracks widget displays the state of a number of tracks.
Item {
    id: tracks
    width: childrenRect.width
    height: childrenRect.height

    property var track_names: []
    property var loop_names: []
    property int loops_per_track
    property var loop_managers //2D array
    property var master_loop_manager
    property var master_loop_idx //[track][loop]
    property var selected_loops

    //Arrays of [track, loop]
    property var loops_of_selected_scene: []
    property var loops_of_hovered_scene: []

    signal request_bind_loop_to_scene(int track, int loop)
    signal request_rename(int track, string name)
    signal request_select_loop(int track, int loop)
    signal request_load_wav(int track, int loop, string wav_file)
    signal request_save_wav(int track, int loop, string wav_file)
    signal request_rename_loop(int track, int loop, string name)
    signal request_clear_loop(int track, int loop)

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
                        loop_names: tracks.loop_names[index]
                        num_loops: tracks.loops_per_track
                        first_index: index * tracks.loops_per_track
                        track_index: index + 1
                        maybe_master_loop_idx: tracks.master_loop_idx[0] === index ? tracks.master_loop_idx[1] : -1
                        master_loop_manager: tracks.master_loop_manager
                        loop_managers: tracks.loop_managers[index]
                        selected_loop: tracks.selected_loops[index]

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
                        onRequest_select_loop: (idx) => tracks.request_select_loop(index, idx)
                        onRequest_load_wav: (idx, wav_file) => tracks.request_load_wav(index, idx, wav_file)
                        onRequest_save_wav: (idx, wav_file) => tracks.request_save_wav(index, idx, wav_file)
                        onRequest_rename_loop: (idx, name) => tracks.request_rename_loop(index, idx, name)
                        onRequest_clear_loop: (idx) => tracks.request_clear_loop(index, idx)
                    }
                }
            }
        }
    }
}
