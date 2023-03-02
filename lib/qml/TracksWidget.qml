import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

import "../generate_session.js" as GenerateSession

ScrollView {
    ScrollBar.horizontal.policy: ScrollBar.AsNeeded
    ScrollBar.vertical.policy: ScrollBar.AlwaysOff
    ScrollBar.horizontal.height: 10
    contentWidth: tracks_view.width
    property int scroll_offset : ScrollBar.horizontal.position * contentWidth

    id: root

    property var initial_track_descriptors : null
    property Registry objects_registry : null
    property Registry state_registry : null

    property bool loaded : false
    property int n_loaded : 0

    onLoadedChanged: if(loaded) { console.log("LOADED: TracksWidget") }

    property alias tracks : tracks_row.children

    readonly property var factory : Qt.createComponent("TrackWidget.qml")

    function add_track(properties) {
        if (factory.status == Component.Error) {
            throw new Error("TracksWidget: Failed to load track factory: " + factory.errorString())
        } else if (factory.status != Component.Ready) {
            throw new Error("TracksWidget: Track factory not ready")
        } else {
            var track = factory.createObject(root, properties);
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

    function load() {
        loaded = false
        var _n_loaded = 0
        // Instantiate initial tracks
        root.initial_track_descriptors.forEach(desc => {
            var track = root.add_track({
                initial_descriptor: desc,
                objects_registry: root.objects_registry,
                state_registry: root.state_registry
            });
            track.onLoadedChanged.connect(() => track_loaded_changed(track))
            if (track.loaded) { _n_loaded += 1 }
        })
        n_loaded = _n_loaded
        loaded = Qt.binding(() => { return n_loaded >= initial_track_descriptors.length })
    }

    Component.onCompleted: load()

    ScrollView {
        id: tracks_view
        ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
        ScrollBar.vertical.policy: ScrollBar.AsNeeded
        ScrollBar.vertical.width: 10
        ScrollBar.vertical.x: root.width-ScrollBar.vertical.width+root.scroll_offset

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
                var idx = (root.tracks.length).toString()
                var track_descriptor = GenerateSession.generate_default_track("Track " + idx, 1, 'track_' + idx)
                root.add_track({
                    initial_descriptor: track_descriptor,
                    objects_registry: root.objects_registry,
                    state_registry: root.state_registry
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
                for(var i=root.tracks.length-1; i>0; i--) {
                    root.tracks[i].qml_close()
                }
                root.tracks.length = 1;
            }
        }
    }
}