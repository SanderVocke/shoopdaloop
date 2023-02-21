import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

// The tracks widget displays the state of a number of tracks.
Item {
    id: tracks
    width: childrenRect.width
    height: childrenRect.height

    property int slots_per_track
    property int n_tracks
    property LoopWidget master_loop
    property LoopWidget targeted_loop

    property list<LoopWidget> loops_of_selected_scene: []
    property list<LoopWidget> loops_of_hovered_scene: []

    // signal request_toggle_loop_in_scene(int track, int loop)
    // signal request_rename(int track, string name)
    // signal request_select_loop(int track, int loop)
    // signal request_rename_loop(int track, int loop, string name)
    // signal request_clear_loop(int track, int loop)
    // signal request_toggle_loop_selected(int track, int loop)
    // signal request_set_targeted_loop(int track, int loop)

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
                    model: tracks.n_tracks
                    id: tracks_repeater

                    TrackWidget {
                        num_slots: tracks.slots_per_track

                        master_loop: tracks.master_loop
                        targeted_loop: tracks.targeted_loop

                        name: 'Track ' + (index + 1).toString()
                    }
                }
            }
        }
    }
}
