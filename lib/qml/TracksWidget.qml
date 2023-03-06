import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

import "../generate_session.js" as GenerateSession

ScrollView {
    ScrollBar.horizontal.policy: ScrollBar.horizontal.size < 1.0 ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
    ScrollBar.vertical.policy: ScrollBar.AlwaysOff
    ScrollBar.horizontal.height: 10
    contentWidth: buttons_column.x + buttons_column.width
    property int scroll_offset : ScrollBar.horizontal.position * contentWidth

    id: root

    property var initial_track_descriptors : null
    property Registry objects_registry : null
    property Registry state_registry : null

    property bool loaded : false
    property int n_loaded : 0

    property alias tracks : tracks_row.children
    property var track_initial_descriptors : []

    readonly property var factory : Qt.createComponent("TrackWidget.qml")

    function add_track(properties) {
        if (factory.status == Component.Error) {
            throw new Error("TracksWidget: Failed to load track factory: " + factory.errorString())
        } else if (factory.status != Component.Ready) {
            throw new Error("TracksWidget: Track factory not ready")
        } else {
            var track = factory.createObject(root, properties);
            track.onLoadedChanged.connect(() => track_loaded_changed(track))
            track.onRowAdded.connect(() => handle_row_added(track))
            root.track_initial_descriptors.push(properties.initial_descriptor)
            root.tracks.push(track)
            return track
        }
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

    Component.onCompleted: {
        loaded = false
        var _n_loaded = 0
        // Instantiate initial tracks
        root.initial_track_descriptors.forEach(desc => {
            var track = root.add_track({
                initial_descriptor: desc,
                objects_registry: root.objects_registry,
                state_registry: root.state_registry
            });
            if (track.loaded) { _n_loaded += 1 }
        })
        n_loaded = _n_loaded
        loaded = Qt.binding(() => { return n_loaded >= tracks.length })
    }

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

            // Note: tracks injected here
        }
    }

    Row {
        id: track_controls_row
        spacing: tracks_row.spacing
        width: tracks_view.width
        anchors.left: tracks_view.left
        anchors.bottom: parent.bottom

        Repeater {
            model: tracks_row.children.length

            Item {
                width: tracks_row.children[index].width
                height: 50

                TrackControlWidget {
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.horizontalCenter: parent.horizontalCenter
                    initial_track_descriptor: root.track_initial_descriptors[index]
                    objects_registry: root.objects_registry
                    state_registry: root.state_registry
                }
            }
        }
    }

    Row {
        id: rectangles_row
        z: -1
        spacing: tracks_row.spacing
        width: tracks_view.width
        anchors.top: parent.top
        anchors.bottom: parent.bottom

        Repeater {
            model: tracks_row.children.length

            Rectangle {
                width: tracks_row.children[index].width
                height: rectangles_row.height
                y: 0
                color: "#555555"
            }
        }
    }


    // Buttons for add / clear
    Column {
        id : buttons_column
        spacing: 3

        anchors {
            top: parent.top
            left: rectangles_row.right
            leftMargin: 6
        }

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
                var idx = (root.tracks.length).toString()
                var track_descriptor = GenerateSession.generate_default_track("Track " + idx, root.max_slots(), 'track_' + idx)
                root.add_track({
                    initial_descriptor: track_descriptor,
                    objects_registry: root.objects_registry,
                    state_registry: root.state_registry
                })
            }
        }
    }
}