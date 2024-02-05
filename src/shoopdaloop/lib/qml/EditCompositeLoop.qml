import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3
import QtQuick.Shapes 6.3

Item {
    id: root
    
    property var loop // LoopWidget
    property var composite_loop : loop.maybe_composite_loop

    property int cycle_width: 130
    readonly property int cycle_length: composite_loop.sync_length
    property int swimlane_height: 34

    property var scheduled_playlists : composite_loop.scheduled_playlists
    property int schedule_length : Math.max.apply(null, Object.keys(composite_loop.schedule))

    property var sync_track
    property var main_tracks : []

    Column {
        id: tracks_mapper
        spacing: 1
        width: parent.width
        height: childrenRect.height

        Track {
            sync_track: true
            track: root.sync_track
            width: tracks_mapper.width
        }

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
        property var loop_widget
        property string loop_id
        property int start_iteration
        property int end_iteration
        property int delay
        property var incoming_edge // refer to other PlaylistElement
        property var outgoing_edge // refer to other PlaylistElement

        // Calculate
        property int swimlane // swimlane where the element should be rendered in
                              // its resp. track

        // set when a GUI item representing this element is created
        property var gui_item
    }
    Component {
        id: playlist_element_factory
        PlaylistElement { }
    }
    
    // Holds a structure equal to that of scheduled_playlists, but with
    // PlaylistElements instead of plain objects.
    // Also calculate the swimlanes along the way.
    readonly property var playlist_elems : {
        // Create the PlaylistElements and populate basic properties
        var rval = []
        let _scheduled_playlists = Array.from(scheduled_playlists.map(a => Array.from(a))) // shallow copy
        let _schedule_length = schedule_length
        for(var i=0; i<_scheduled_playlists.length; i++) {
            var playlist = []
            var prev_elem = null
            for(var j=0; j<_scheduled_playlists[i].length; j++) {
                let ori = _scheduled_playlists[i][j]
                let elem = playlist_element_factory.createObject(root, {
                    loop_widget: ori.loop_widget,
                    loop_id: ori.loop_id,
                    start_iteration: ori.start_iteration,
                    end_iteration: ori.end_iteration,
                    delay: ori.delay,
                    swimlane: -1
                })
                if (prev_elem) {
                    prev_elem.outgoing_edge = elem
                    elem.incoming_edge = prev_elem
                }
                prev_elem = elem
                playlist.push(elem)
            }
            rval.push(playlist)
        }

        // Now calculate the swimlanes
        let create_swimlane_slot_status = () => new Array(_schedule_length).fill(false)
        // Start with one swimlane per track
        var swimlanes_per_track = new Array(main_tracks.length).fill(null).map(() => [create_swimlane_slot_status()])
        let is_free = (track_idx, swimlane_idx, start_iter, end_iter) => {
            for(var j=start_iter; j<end_iter; j++) {
                if (swimlanes_per_track[track_idx][swimlane_idx][j]) { return false; }
            }
            return true;
        }
        // Start filling the swimlanes
        for (var i=0; i<rval.length; i++) {
            let playlist = rval[i]
            for (var j=0; j<playlist.length; j++) {
                let elem = playlist[j]
                var swimlane = -1
                let track_idx = elem.loop_widget.track_idx
                let swimlanes = swimlanes_per_track[track_idx]

                // Check the existing swimlanes first
                var check_swimlanes = (new Array(swimlanes.length).fill(0)).map((v, idx) => idx) // in ascending order
                if (elem.incoming_edge && elem.incoming_edge.loop_widget.track_idx == elem.loop_widget.track_idx) {
                    // If there is a preceding element in the same track, prefer to go in the same swimlane
                    check_swimlanes.splice(0, 0, elem.incoming_edge.swimlane)
                }
                for(var k=0; k<check_swimlanes.length; k++) {
                    if (is_free(track_idx, check_swimlanes[k], elem.start_iteration, elem.end_iteration)) {
                        swimlane = check_swimlanes[k]
                        break;
                    }
                }
                // Create new swimlane if needed
                if (swimlane == -1) {
                    // new swimlane needed
                    swimlanes.push(create_swimlane_slot_status())
                    swimlane = swimlanes.length - 1
                }

                // Mark the newly taken slots
                for(var h=elem.start_iteration; h<elem.end_iteration; h++) {
                    swimlanes[swimlane][h] = true
                }

                elem.swimlane = swimlane
            }
        }

        return rval
    }

    // Flatten the elements-based playlists into a list of all elements.
    readonly property var flat_playlist_elems : {
        var rval = []
        for(var i=0; i<playlist_elems.length; i++) {
            for(var j=0; j<playlist_elems[i].length; j++) {
                rval.push(playlist_elems[i][j])
            }
        }
        return rval
    }

    // Take playlists with PlaylistElements, translate back to the
    // format used by CompositeLoop and push to the CompositeLoop
    function push_playlists(elems_playlists) {
        let item_from_elem = (elem) => {
            return {
                'delay': elem.delay,
                'loop_id': elem.loop_id,
            }
        }
        var playlists = []
        for(var i=0; i<elems_playlists.length; i++) {
            let playlist = []
            for(var j=0; j<elems_playlists[i].length; j++) {
                playlist.push(item_from_elem(elems_playlists[i][j]))
            }
            playlists.push(playlist)
        }
        composite_loop.playlists = playlists
    }

    // Delete PlaylistElement from the schedule and push
    function delete_elem_and_push(elem) {
        // Delete the element from the schedule
        let prev = elem.incoming_edge
        let next = elem.outgoing_edge
        if (prev && next) {
            prev.outgoing_edge = next
            next.incoming_edge = prev
        } else if (prev) {
            prev.outgoing_edge = null
        } else if (next) {
            next.incoming_edge = null
        }
        let new_elems_schedule = playlist_elems.map(p => p.filter(e => e != elem))
        push_playlists(new_elems_schedule)
    }

    // Add an element connected after an existing one and push
    function add_connected_elem_and_push(existing_elem, new_elem, delay) {
        new_elem.incoming_edge = existing_elem
        if (existing_elem.outgoing_edge) {
            // Insert in-between in case there is already a connected item
            new_elem.outgoing_edge = existing_elem.outgoing_edge
            new_elem.outgoing_edge.incoming_edge = new_elem
        }
        existing_elem.outgoing_edge = new_elem
        new_elem.delay = delay
        new_elem.start_iteration = existing_elem.end_iteration + delay
        new_elem.end_iteration = new_elem.start_iteration + new_elem.loop_widget.n_cycles
        var new_elems_schedule = []
        for (var i=0; i<playlist_elems.length; i++) {
            let playlist = playlist_elems[i]
            let new_playlist = []
            for (var j=0; j<playlist.length; j++) {
                let elem = playlist[j]
                if (elem == existing_elem) {
                    new_playlist.push(elem)
                    new_playlist.push(new_elem)
                } else {
                    new_playlist.push(elem)
                }
            }
            new_elems_schedule.push(new_playlist)
        }
        push_playlists(new_elems_schedule)
    }

    // Create a new playlist with a single element in it.
    function add_new_playlist_with_elem_and_push(new_elem, delay) {
        new_elem.start_iteration = delay
        new_elem.end_iteration = new_elem.start_iteration + new_elem.loop_widget.n_cycles
        var new_elems_schedule = []
        for (var i=0; i<playlist_elems.length; i++) {
            let playlist = playlist_elems[i]
            let new_playlist = []
            for (var j=0; j<playlist.length; j++) {
                let elem = playlist[j]
                new_playlist.push(elem)
            }
            new_elems_schedule.push(new_playlist)
        }
        new_elems_schedule.push([new_elem])
        push_playlists(new_elems_schedule)
    }

    component Track : Item {
        id: track_root

        property var track
        readonly property int track_idx : track.track_idx

        // The sync track is shown differently, and is not editable.
        // We do use this component for it so that we can nicely render
        // everything in an aligned way.
        property bool sync_track
        
        height: childrenRect.height

        // Filter playlist elements that belong to this track.
        readonly property var track_playlist_elems :
            sync_track ? null :
            root.flat_playlist_elems.filter(e => e.loop_widget.track_idx == track_idx)

        // Find the amount of swimlanes needed.
        readonly property int n_swimlanes: 
            sync_track ? 0 :
            Math.max.apply(null, track_playlist_elems.map(l => l.swimlane)) + 1

        Label {
            id: name_label
            anchors {
                left: parent.left
                top: content_rect.top
                bottom: content_rect.bottom
            }
            text: track_root.sync_track ? 'Cycle' : track.name
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
                leftMargin: 6
            }
            height: Math.max(childrenRect.height, root.swimlane_height)
            color: track_root.sync_track ? 'transparent' : Material.background

            Loader {
                active: track_root.sync_track
                id: show_sync_track

                sourceComponent: Component { Repeater {
                    model: Math.max(root.schedule_length, (content_rect.width + root.cycle_width) / root.cycle_width)
                    height: childrenRect.height
                    width: childrenRect.width
                    Rectangle {
                        color: 'transparent'
                        width: root.cycle_width
                        height: root.swimlane_height
                        border.color: Material.foreground
                        border.width: 1
                        x: index * root.cycle_width

                        Label {
                            anchors.fill: parent
                            horizontalAlignment: Text.AlignHCenter
                            verticalAlignment: Text.AlignVCenter
                            text: index
                        }

                        DropArea {
                            keys: ["LoopWidget"]
                            anchors.fill: parent

                            onDropped: (event) => {
                                let src_loop_widget = drag.source
                                let new_elem = playlist_element_factory.createObject(root, {
                                    loop_widget: src_loop_widget,
                                    loop_id: src_loop_widget.obj_id,
                                    incoming_edge: null,
                                    outgoing_edge: null,
                                    delay: index
                                })
                                add_new_playlist_with_elem_and_push(new_elem, index)
                            }

                            Rectangle {
                                anchors.fill: parent
                                opacity: 0.3
                                visible: parent.containsDrag
                            }
                        }
                    }
                }}
            }

            Loader {
                active: track_root.n_swimlanes && track_root.n_swimlanes > 0
                id: show_track

                sourceComponent: Component { Item {
                    id: swimlanes_column
                    
                    property int spacing: 1
                    
                    height: childrenRect.height
                    width: root.cycle_width * track_root.track_schedule_length

                    Repeater {
                        id: swimlanes
                        model: track_root.n_swimlanes

                        // background rectangle for the entire swimlane.
                        Rectangle {
                            id: swimlane
                            border.color: 'black'
                            border.width: 1
                            color: 'transparent'

                            width: swimlanes_column.width
                            height: root.swimlane_height
                            y: index * (root.swimlane_height + swimlanes_column.spacing)

                            Mapper {
                                model : track_root.track_playlist_elems.filter(l => l.swimlane == index)

                                // Rectangle representing a loop on the schedule.
                                Rectangle {
                                    id: loop_rect
                                    property var mapped_item
                                    property int index

                                    color: 'grey'
                                    border.color: 'black'
                                    border.width: 1

                                    width: root.cycle_width * (mapped_item.end_iteration - mapped_item.start_iteration)
                                    height: swimlane.height
                                    x: root.cycle_width * mapped_item.start_iteration

                                    Label {
                                        anchors.centerIn: parent
                                        text: mapped_item.loop_widget.name
                                    }

                                    Component.onCompleted: {
                                        mapped_item.gui_item = this
                                    }

                                    ExtendedButton {
                                        tooltip: "Delete from the sequence"

                                        height: 24
                                        width: 24
                                        onClicked: {
                                            root.delete_elem_and_push(mapped_item)
                                        }

                                        MaterialDesignIcon {
                                            size: Math.min(parent.width, parent.height) - 10
                                            anchors.centerIn: parent
                                            name: 'delete'
                                            color: Material.foreground
                                        }
                                    }

                                    // If this loop is preceding another in the playlist, show that
                                    // by a "link icon" to the next one.
                                    LinkIndicator {
                                        visible: loop_rect.mapped_item.outgoing_edge != null
                                        height: loop_rect.height / 2
                                        width: height
                                        color: 'white'
                                        side: 'right'

                                        anchors {
                                            right: parent.right
                                            verticalCenter: parent.verticalCenter
                                        }
                                    }

                                    // Some for incoming connections
                                    LinkIndicator {
                                        visible: loop_rect.mapped_item.incoming_edge != null
                                        height: loop_rect.height / 2
                                        width: height
                                        color: 'white'
                                        side: 'left'

                                        anchors {
                                            left: parent.left
                                            verticalCenter: parent.verticalCenter
                                        }
                                    }

                                    // DropArea for connecting a loop to be sequenced after this one
                                    DropArea {
                                        keys: ["LoopWidget"]
                                        anchors {
                                            right: parent.right
                                            left: parent.horizontalCenter
                                            top: parent.top
                                            bottom: parent.bottom
                                        }

                                        onDropped: (event) => {
                                            let src_loop_widget = drag.source
                                            let new_elem = playlist_element_factory.createObject(root, {
                                                loop_widget: src_loop_widget,
                                                loop_id: src_loop_widget.obj_id
                                            })
                                            add_connected_elem_and_push(mapped_item, new_elem, 0)
                                        }

                                        Rectangle {
                                            anchors.fill: parent
                                            opacity: 0.3
                                            visible: parent.containsDrag
                                        }
                                    }
                                }
                            }
                        }
                    }
                }}
            }

            Rectangle {
                id: play_head
                width: 3
                height: Math.max(show_sync_track.height, show_track.height)
                color: 'green'
                visible: parseInt(x) != 0

                x: (composite_loop.position * root.cycle_width) / root.cycle_length
            }
        }
    }

    // Indicator shown on the sides of scheduled loop elements
    // to show they are linked to other loops sequentially.
    component LinkIndicator : Item {
        id: indicator

        property var color : 'purple'
        property string side : 'left' // or 'right'

        readonly property bool flip : side == 'right'

        Shape {
            anchors.fill: parent

            // flip transform
            transform : Scale {
                xScale:   indicator.flip ? -1 : 1
                origin.x: indicator.flip ? width/2 : 0 
            }

            ShapePath {
                startX: 0
                startY: 0
                //fillColor: indicator.color

                PathArc {
                    x: 0
                    y: parent.height / 2
                    radiusX: parent.height / 4
                    radiusY: parent.height / 4
                    useLargeArc: true
                }
                PathLine { x: 0; y: 0 }
            }
        }
    }
}