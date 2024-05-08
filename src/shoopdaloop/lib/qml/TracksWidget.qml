import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6

import "../generate_session.js" as GenerateSession

import ShoopDaLoop.PythonLogger

ScrollView {
    ScrollBar.horizontal.policy: ScrollBar.horizontal.size < 1.0 ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
    ScrollBar.vertical.policy: ScrollBar.AlwaysOff
    ScrollBar.horizontal.height: 10
    contentWidth: buttons_column.x + buttons_column.width
    property int scroll_offset : ScrollBar.horizontal.position * contentWidth

    property PythonLogger logger : PythonLogger { name: "Frontend.Qml.TracksWidget" }

    id: root

    property var initial_track_descriptors : []

    property bool loaded : false
    property int n_loaded : 0

    property var tracks : []
    property var track_initial_descriptors : []

    onTracksChanged: {
        // Keep indexes up to date
        tracks.forEach((c, idx) => { c.track_idx = idx } )
    }

    RegisterInRegistry {
        registry: registries.state_registry
        key: 'track_descriptors'
        object: track_initial_descriptors
    }

    readonly property var factory : Qt.createComponent("TrackWidget.qml")

    function actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to) {
        var r = []
        for(var i=0; i<root.tracks.length; i++) {
            r.push(tracks[i].actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to))
        }
        return r;
    }

    function queue_load_tasks(data_files_dir, from_sample_rate, to_sample_rate, add_tasks_to) {
        root.logger.debug(() => (`Queue load tasks for ${root.tracks.length} tracks`))
        for(var i=0; i<root.tracks.length; i++) {
            tracks[i].queue_load_tasks(data_files_dir, from_sample_rate, to_sample_rate, add_tasks_to)
        }
    }

    function add_track(properties) {
        if (factory.status == Component.Error) {
            throw new Error("TracksWidget: Failed to load track factory: " + factory.errorString())
        } else if (factory.status != Component.Ready) {
            throw new Error("TracksWidget: Track factory not ready")
        } else {
            var track = factory.createObject(root, properties);
            track.onLoadedChanged.connect(() => track_loaded_changed(track))
            track.onRowAdded.connect(() => handle_row_added(track))
            track.onRequestDelete.connect(() => delete_track(track))
            root.track_initial_descriptors.push(properties.initial_descriptor)
            root.tracks.push(track)
            root.tracksChanged()
            root.track_initial_descriptorsChanged()

            return track
        }
    }

    function delete_track(track) {
        var new_tracks = []
        for(var i=0; i<root.tracks.length; i++) {
            if (root.tracks[i] != track) { new_tracks.push(root.tracks[i]) }
        }
        track.qml_close()
        root.tracks = new_tracks
    }

    function clear_tracks() {
        for(var i=0; i<root.tracks.length; i++) {
            root.tracks[i].qml_close()
        }
        root.tracks = []
    }

    function track_loaded_changed(track) {
        if(track.loaded) {
            n_loaded = n_loaded + 1;
        } else {
            n_loaded = n_loaded - 1;
        }
    }

    function max_slots() {
        var max = 0
        for (var i=0; i<tracks.length; i++) {
            max = Math.max(max, tracks[i].num_slots)
        }
        return max
    }

    function handle_row_added(track) {
        // If a row is added to a track which has the highest amount of rows,
        // also add one to the other tracks with the same amount
        var max = max_slots()
        if (track.num_slots == max) {
            var prev = track.num_slots - 1
            for (var i=0; i<tracks.length; i++) {
                if(tracks[i].num_slots == prev) { tracks[i].add_row() }
            }
        }
    }

    function initialize() {
        loaded = false
        root.clear_tracks()
        var _n_loaded = 0
        // Instantiate initial tracks
        root.initial_track_descriptors.forEach(desc => {
            var track = root.add_track({
                initial_descriptor: desc
            });
            if (track.loaded) { _n_loaded += 1 }
        })
        n_loaded = _n_loaded
        loaded = Qt.binding(() => { return n_loaded >= tracks.length })
    }

    Component.onCompleted: initialize()
    function reload() { initialize() }

    ScrollView {
        id: tracks_view
        ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
        ScrollBar.vertical.policy: ScrollBar.vertical.size < 1.0 ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff;
        ScrollBar.vertical.width: 10
        ScrollBar.vertical.x: Math.min(root.width-ScrollBar.vertical.width+root.scroll_offset, root.contentWidth)

        anchors.left: parent.left
        anchors.top: parent.top
        anchors.bottom: track_controls_row.top
        width: tracks_row.width

        Row {
            spacing: 3
            id: tracks_row
            width: childrenRect.width
            anchors {
                top: parent.top
                bottom: parent.bottom
            }
            
            Mapper {
                id: tracks_mapper
                model: root.tracks

                Item {
                    height: tracks_row.height

                    property var mapped_item
                    property int index

                    children: [mapped_item]
                    width: childrenRect.width
                }
            }
        }

        // Repeater for the drag'n'drop drop areas to drop tracks in.
        // These drop areas are in-between the tracks and can be used to reorder them.
        Repeater {
            id: drop_areas_repeater
            anchors.fill: tracks_row
            model: tracks_mapper.model.length + 1

            DropArea {
                id: drop_area
                width: 50
                height: tracks_row.height
                keys: ['TrackWidget']

                property var left_track : index > 0 ? tracks_mapper.sorted_instances[index-1] : null
                property var right_track : index < tracks_mapper.sorted_instances.length ? tracks_mapper.sorted_instances[index] : null

                onDropped: (event) => {
                    let src_track = drag.source
                    let src_track_idx = src_track.track_idx
                    let tracks = root.tracks
                    var new_tracks = []
                    for (var i=0; i<tracks.length+1; i++) {
                        if (i == index) {
                            new_tracks.push(src_track)
                        }
                        if (i != src_track_idx && i < tracks.length) {
                            new_tracks.push(tracks[i])
                        }
                    }
                    root.tracks = new_tracks
                }

                x : {
                    if (left_track) {
                        left_track.x; // dummy dependency
                        return left_track.mapToItem(tracks_view, 0, 0).x + left_track.width + tracks_row.spacing/2 - width/2
                    }
                    if (right_track) {
                        right_track.x; // dummy dependency
                        return right_track.mapToItem(tracks_view, 0, 0).x - tracks_row.spacing/2 - width/2
                    }
                    return 0
                }
                y : 0

                Rectangle {
                    color: 'white'
                    opacity: 0.3

                    visible : (parent.left_track || parent.right_track) && drop_area.containsDrag

                    anchors.fill: parent
                }

            }
        }
    }

    Row {
        id: track_controls_row
        spacing: tracks_row.spacing
        width: tracks_view.width
        anchors.left: tracks_view.left
        anchors.bottom: parent.bottom

        Mapper {
            id: track_controls_mapper
            model: root.tracks

            Item {
                id: a_track
                property var mapped_item
                property int index
                property var widget: widget

                width: mapped_item.width
                height: 50

                TrackControlWidget {
                    id: widget
                    width: parent.width
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.horizontalCenter: parent.horizontalCenter
                    initial_track_descriptor: a_track.mapped_item.initial_descriptor
                }
            }
        }
    }
    function get_track_control_widget(track_idx) {
        return track_controls_mapper.unsorted_instances[track_idx].widget
    }
    function get_track_control_widgets() {
        return track_controls_mapper.unsorted_instances.map((item) => item.widget)
    }

    Row {
        id: rectangles_row
        z: -1
        spacing: tracks_row.spacing
        width: tracks_view.width
        anchors.top: parent.top
        anchors.bottom: parent.bottom

        Mapper {
            model: root.tracks

            Rectangle {
                property var mapped_item
                property int index

                width: mapped_item.width
                height: rectangles_row.height
                y: 0
                color: "#555555"
            }
        }
    }

    // Button for add track
    Column {
        id : buttons_column
        spacing: 3

        anchors {
            top: parent.top
            left: rectangles_row.right
            leftMargin: 6
        }

        ExtendedButton {
            tooltip: "Create a new track."
            width: 30
            height: 40
            MaterialDesignIcon {
                size: 20
                name: 'plus'
                color: Material.foreground
                anchors.centerIn: parent
            }
            onClicked: newtrackdialog.open_for_new_track()
        }
    }

    // Dialog for adding a new track
    NewTrackDialog {
        id: newtrackdialog
        tracks: root.tracks
        get_max_loop_slots: () => root.max_slots()
        onAddTrackDescriptor: (desc) => {
            root.add_track({
                initial_descriptor: desc
            })
        }
    }
}