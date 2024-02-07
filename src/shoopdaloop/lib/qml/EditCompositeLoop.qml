import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3
import QtQuick.Shapes 6.3

Item {
    id: root
    
    property var loop // LoopWidget
    property var composite_loop : loop.maybe_composite_loop

    property alias cycle_width: zoom_slider.value
    property int x_offset: 0

    readonly property int cycle_length: composite_loop.sync_length
    property int swimlane_height: 28

    property var scheduled_playlists : composite_loop.scheduled_playlists
    property int schedule_length : Math.max.apply(null, Object.keys(composite_loop.schedule))

    property var sync_track
    property var main_tracks : []

    height: childrenRect.height

    Row {
        id: toolbar
        property int tool_buttons_size: 24

        anchors {
            top: parent.top
            left: parent.left
        }
        height: 40
        spacing: 2

        ToolbarButton {
            anchors {
                verticalCenter: parent.verticalCenter
            }
            material_design_icon: 'magnify'
            size: toolbar.tool_buttons_size

            onClicked: zoom_popup.open()
            toggle_visual_active: zoom_popup.visible

            Popup {
                id: zoom_popup
                leftInset: 20
                rightInset: 50
                topInset: 20
                bottomInset: 20

                ShoopSlider {
                    id: zoom_slider
                    width: 160
                    value: 130
                    from: 20
                    to: 600

                    anchors {
                        verticalCenter: parent.verticalCenter
                    }
                }
            }
        }

        property real zoom_step_amount : 0.05 * (zoom_slider.to - zoom_slider.from)
        ToolbarButton {
            anchors {
                verticalCenter: parent.verticalCenter
            }
            material_design_icon: 'magnify-plus-outline'
            size: toolbar.tool_buttons_size

            onClicked: zoom_slider.value = Math.min(zoom_slider.value + toolbar.zoom_step_amount, zoom_slider.to)
        }

        ToolbarButton {
            anchors {
                verticalCenter: parent.verticalCenter
            }
            material_design_icon: 'magnify-minus-outline'
            size: toolbar.tool_buttons_size

            onClicked: zoom_slider.value = Math.max(zoom_slider.value - toolbar.zoom_step_amount, zoom_slider.from)
        }

        ToolbarButton {
            anchors {
                verticalCenter: parent.verticalCenter
            }
            material_design_icon: 'magnify'
            size: toolbar.tool_buttons_size

            Label {
                x: 7
                y: 4
                text: "A"
                color: Material.foreground
                font.pixelSize: 8
            }

            onClicked: {
                root.x_offset = 0
                root.cycle_width = tracks_column.width / root.schedule_length - 5
            }
        }
    }

    Rectangle {
        id: all_content
        anchors {
            top: toolbar.bottom
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }

        // Backgrounds of each track are rendered here, underneath our drop areas
        Item {
            anchors.fill: tracks_column
            clip: true

            Mapper {
                model : {
                    var rval = Array.from(tracks_mapper.sorted_instances)
                    rval.push(cycle_header)
                    return rval
                }

                Rectangle {
                    property int index
                    property var mapped_item

                    width: mapped_item.width
                    height: mapped_item.height
                    y: {
                        mapped_item.y; //dummy dependency
                        return mapped_item.mapToItem(tracks_column, 0, 0).y
                    }
                    x: {
                        mapped_item.x; //dummy dependency
                        return mapped_item.mapToItem(tracks_column, 0, 0).x
                    }

                    color: Material.background
                }
            }
        }

        // Overlay drop areas to drop a loop in a cycle
        Item {
            anchors.fill: tracks_column

            Mapper {
                model : cycle_header.children.filter(c => !(c instanceof Repeater))

                DropArea {
                    property int index
                    property var mapped_item

                    keys: ["LoopWidget"]
                    
                    width: mapped_item.width
                    y: 0
                    height: tracks_column.height
                    x: {
                        mapped_item.x; //dummy dependency
                        return mapped_item.mapToItem(tracks_column, 0, 0).x
                    }

                    onDropped: (event) => {
                        let src_loop_widget = drag.source
                        let new_elem = playlist_element_factory.createObject(root, {
                            loop_widget: src_loop_widget,
                            loop_id: src_loop_widget.obj_id,
                            incoming_edge: null,
                            outgoing_edge: null,
                            delay: mapped_item.cycle
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
        }

        Column {
            id: tracks_column
            spacing: 1
            anchors {
                left: track_labels.right
                right: parent.right
                top: parent.top
            }
            height: childrenRect.height

            // Header showing the current cycle
            Rectangle {
                id: cycle_header
                clip: true
                anchors {
                    left: parent.left
                    right: parent.right
                }
                height: 16
                color: 'transparent'

                property int first_cycle_rendered : Math.floor(root.x_offset / root.cycle_width)
                property int first_cycle_pos : first_cycle_rendered*root.cycle_width - root.x_offset
                property int render_n_cycles : (width / root.cycle_width) + 2
               
                Repeater {
                    model : parent.render_n_cycles

                    Rectangle {
                        property int cycle : cycle_header.first_cycle_rendered + index

                        color: 'transparent'
                        border.color: Material.foreground
                        border.width: 1
                        width: root.cycle_width
                        x: cycle_header.first_cycle_pos + root.cycle_width * index
                        anchors {
                            top: parent.top
                            bottom: parent.bottom
                        }

                        Label {
                            anchors.centerIn: parent
                            text: parent.cycle + 1
                        }
                    }
                }
            }

            Mapper {
                model : main_tracks
                id: tracks_mapper
                Track {
                    property var mapped_item
                    property int index

                    track : mapped_item
                    width : tracks_column.width
                    x_offset : root.x_offset
                }
            }
        }

        Item {
            id: track_labels
            width: childrenRect.width + 6
            height: tracks_column.height
            y: tracks_column.y

            Mapper {
                model: main_tracks

                Label {
                    property int index
                    property var mapped_item
                    property var visual_track : {
                        let r = tracks_mapper.sorted_instances.filter(i => i.mapped_item == mapped_item)
                        if (r.length == 1) { return r[0]; }
                        return undefined;
                    }

                    y: visual_track ? visual_track.y + visual_track.height/2 - height/2 : 0
                    height: visual_track ? visual_track.height : undefined
                    verticalAlignment: Text.AlignVCenter

                    text: mapped_item.name
                }
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
        property var maybe_forced_n_cycles
        property var incoming_edge // refer to other PlaylistElement
        property var outgoing_edge // refer to other PlaylistElement
        property color incoming_edge_color
        property color outgoing_edge_color
        property var info

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
    
    // Calculate all the information we need to turn the composite loop
    // playlist items into fully annotated PlaylistElement objects.
    // Holds a structure equal to that of scheduled_playlists.
    readonly property var playlist_elem_placeholders : {
        // First some logic for picking colors for the inter-loop links.
        let colors = [
            '#da7454',
	        '#8ac4ae',
	        '#eebe60',
	        '#f4ede0',
	        '#7685c0'
        ]
        var color_idx = 0
        function pick_color() {
            let rval = colors[color_idx]
            color_idx = (color_idx + 1) % colors.length
            return rval
        }

        // Create placeholders holding the basic properties
        // that will later become PlaylistElements
        var placeholders = []
        let _scheduled_playlists = Array.from(scheduled_playlists.map(a => Array.from(a))) // shallow copy
        let _schedule_length = schedule_length
        for(var i=0; i<_scheduled_playlists.length; i++) {
            var playlist = []
            var prev_elem = null
            for(var j=0; j<_scheduled_playlists[i].length; j++) {
                let ori = _scheduled_playlists[i][j]
                let elem = {
                    "ori_elem": ori,
                    "swimlane": -1,
                    "incoming_edge": null,
                    "outgoing_edge": null,
                    "incoming_edge_color": 'grey',
                    "outgoing_edge_color": 'grey'
                }
                if (prev_elem) {
                    prev_elem.outgoing_edge = elem
                    elem.incoming_edge = prev_elem
                    let c = pick_color()
                    prev_elem.outgoing_edge_color = c
                    elem.incoming_edge_color = c
                }
                playlist.push(elem)
                prev_elem = playlist[playlist.length - 1]
            }
            placeholders.push(playlist)
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
        for (var i=0; i<placeholders.length; i++) {
            let playlist = placeholders[i]
            for (var j=0; j<playlist.length; j++) {
                let elem = playlist[j]
                var swimlane = -1
                let loop_widget = elem.ori_elem.loop_widget
                let track_idx = loop_widget.track_idx
                let swimlanes = swimlanes_per_track[track_idx]

                // Check the existing swimlanes first
                var check_swimlanes = (new Array(swimlanes.length).fill(0)).map((v, idx) => idx) // in ascending order
                if (elem.incoming_edge && elem.incoming_edge.ori_elem.loop_widget.track_idx == loop_widget.track_idx) {
                    // If there is a preceding element in the same track, prefer to go in the same swimlane
                    check_swimlanes.splice(0, 0, elem.incoming_edge.swimlane)
                }
                for(var k=0; k<check_swimlanes.length; k++) {
                    if (is_free(track_idx, check_swimlanes[k], elem.ori_elem.start_iteration, elem.ori_elem.end_iteration)) {
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
                for(var h=elem.ori_elem.start_iteration; h<elem.ori_elem.end_iteration; h++) {
                    swimlanes[swimlane][h] = true
                }

                elem.swimlane = swimlane
            }
        }

        // Now instantiate the actual playlist elements
        return placeholders
    }

    // Create the PlaylistElements.
    readonly property var playlist_elems : {
        var rval = []
        for(var i=0; i<playlist_elem_placeholders.length; i++) {
            var playlist = []
            var lookup = {}
            for(var j=0; j<playlist_elem_placeholders[i].length; j++) {
                let info = playlist_elem_placeholders[i][j]
                playlist.push(playlist_element_factory.createObject(root, {
                    loop_widget: info.ori_elem.loop_widget,
                    loop_id: info.ori_elem.loop_id,
                    start_iteration: info.ori_elem.start_iteration,
                    end_iteration: info.ori_elem.end_iteration,
                    delay: info.ori_elem.delay,
                    swimlane: info.swimlane,
                    incoming_edge: j > 0 ? playlist[playlist.length-1] : null, // PlaylistElement previously created
                    outgoing_edge: null, // fill in later
                    incoming_edge_color: info.incoming_edge_color,
                    outgoing_edge_color: info.outgoing_edge_color,
                    maybe_forced_n_cycles: info.ori_elem.forced_n_cycles
                }))
            }
            rval.push(playlist)
        }
        return rval
    }
    // To prevent binding loops, we resolve some of the edges to other PlaylistElements
    // after initial creation.
    onPlaylist_elemsChanged: {
        playlist_elems.forEach(p => p.forEach(e => {
            if (e.incoming_edge) {
                e.incoming_edge.outgoing_edge = e
            }
        }))
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
                'n_cycles': elem.maybe_forced_n_cycles ? elem.maybe_forced_n_cycles : undefined
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

    // Insert an element into an existing playlist
    function add_connected_elem_and_push(existing_elem, new_elem, put_before, delay) {
        if (put_before) {
            new_elem.outgoing_edge = existing_elem
            if (existing_elem.incoming_edge) {
                // Insert in-between in case there is already a connected item
                new_elem.incoming_edge = existing_elem.incoming_edge
                new_elem.incoming_edge.outgoing_edge = new_elem
            }
            existing_elem.incoming_edge = new_elem
            new_elem.start_iteration = existing_elem.start_iteration
            new_elem.end_iteration = new_elem.start_iteration + new_elem.loop_widget.n_cycles
        } else {
            new_elem.incoming_edge = existing_elem
            if (existing_elem.outgoing_edge) {
                // Insert in-between in case there is already a connected item
                new_elem.outgoing_edge = existing_elem.outgoing_edge
                new_elem.outgoing_edge.incoming_edge = new_elem
            }
            existing_elem.outgoing_edge = new_elem
            new_elem.start_iteration = existing_elem.end_iteration + delay
            new_elem.end_iteration = new_elem.start_iteration + new_elem.loop_widget.n_cycles
        }
        new_elem.delay = delay
        var new_elems_schedule = []
        for (var i=0; i<playlist_elems.length; i++) {
            let playlist = playlist_elems[i]
            let new_playlist = []
            for (var j=0; j<playlist.length; j++) {
                let elem = playlist[j]
                if (elem == existing_elem) {
                    if (put_before) {
                        new_playlist.push(new_elem)
                        new_playlist.push(elem)
                    } else {
                        new_playlist.push(elem)
                        new_playlist.push(new_elem)
                    }
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

    // For a particular element, force the amount of cycles to run.
    function force_elem_n_cycles_and_push(elem, n_cycles) {
        elem.maybe_forced_n_cycles = n_cycles
        push_playlists(playlist_elems)
    }

    // Unlink connected loops and push.
    function unlink_and_push(from_elem, to_elem) {
        // To do this we need to cut a playlist in two.
        var new_elems_schedule = []
        playlist_elems.forEach(p => {
            if (!p.includes(from_elem) || !p.includes(to_elem)) { new_elems_schedule.push(Array.from(p)); return; }
            let first_playlist = []
            let second_playlist = []
            var found = false
            for(var i=0; i<p.length; i++) {
                if (found) {
                    second_playlist.push(p[i])
                    if (p[i] == to_elem) {
                        to_elem.delay = to_elem.start_iteration
                        to_elem.incoming_edge = null
                    }
                } else if (p[i] == from_elem) {
                    found = true
                    first_playlist.push(p[i])
                    from_elem.outgoing_edge = null
                } else {
                    first_playlist.push(p[i])
                }
            }
            new_elems_schedule.push(Array.from(first_playlist));
            new_elems_schedule.push(Array.from(second_playlist));
        })
        push_playlists(new_elems_schedule)
    }

    component Track : Item {
        id: track_root

        property var track
        readonly property int track_idx : track.track_idx

        property int x_offset : 0
        
        height: childrenRect.height

        // Filter playlist elements that belong to this track.
        readonly property var track_playlist_elems :
            root.flat_playlist_elems.filter(e => e.loop_widget.track_idx == track_idx)

        // Find the amount of swimlanes needed.
        readonly property int n_swimlanes: 
            Math.max.apply(null, track_playlist_elems.map(l => l.swimlane)) + 1

        Rectangle {
            id: content_rect
            clip: true
            anchors {
                top: parent.top
                left: parent.left
                right: parent.right
            }
            height: Math.max(childrenRect.height, root.swimlane_height)
            color: 'transparent'

            // Handler for panning the view
            DragHandler {
                id: drag_handler
                target: null

                acceptedButtons: Qt.MiddleButton | Qt.RightButton

                xAxis.enabled: true
                yAxis.enabled: false

                property real prev_pos: 0.0

                onActiveChanged: {
                    if (!active) { prev_pos = 0.0 }
                }
                onActiveTranslationChanged: {
                    let val = activeTranslation.x
                    let changed = val - prev_pos
                    root.x_offset -= changed
                    prev_pos = val
                }
            }

            Item {
                width: parent.width
                height: childrenRect.height
                y: 0
                x: -root.x_offset

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
                                            text: {
                                                var rval = mapped_item.loop_widget.name
                                                if (mapped_item.maybe_forced_n_cycles) {
                                                    rval = rval + " (!)"
                                                }
                                                return rval
                                            }
                                        }

                                        Component.onCompleted: {
                                            mapped_item.gui_item = this
                                        }

                                        // Draggy rect for right-side width ajustment
                                        Rectangle {
                                            id: right_width_adjuster
                                            anchors {
                                                right: parent.right
                                                top: parent.top
                                                bottom: parent.bottom
                                            }
                                            width: 20

                                            // for debugging
                                            //color: 'red'
                                            color: 'transparent'

                                            Rectangle {
                                                id: right_width_adjuster_movable
                                                width: right_width_adjuster.width
                                                height: right_width_adjuster.height
                                                x: 0
                                                y: 0
                                                color: "transparent"

                                                Drag.active: right_drag_area.drag.active
                                                visible: Drag.active

                                                // Draw a rectangle that represents the new
                                                // loop length and positioning
                                                Rectangle {
                                                    id: right_resize_preview
                                                    parent: swimlane

                                                    readonly property int dragged_cycle : {
                                                        right_width_adjuster_movable.x; //dummy dependency
                                                        let dragged_x = right_width_adjuster_movable.mapToItem(loop_rect, 0, 0).x
                                                        return Math.ceil(dragged_x / root.cycle_width)
                                                    }

                                                    x: loop_rect.x
                                                    z: 3
                                                    height: loop_rect.height
                                                    width: Math.max(1, dragged_cycle) * root.cycle_width
                                                    color: "blue"
                                                    opacity: 0.3
                                                    visible: right_width_adjuster_movable.visible
                                                }
                                            }

                                            MouseArea {
                                                id: right_drag_area
                                                anchors.fill: parent
                                                cursorShape: Qt.SizeHorCursor

                                                onReleased: {
                                                    if (drag.active) {
                                                        let new_n = right_resize_preview.dragged_cycle
                                                        if (new_n >= 1) {
                                                            force_elem_n_cycles_and_push(loop_rect.mapped_item, new_n)
                                                        }
                                                    }
                                                }

                                                drag {
                                                    axis: "XAxis"
                                                    target: right_width_adjuster_movable
                                                }
                                            }
                                        }

                                        // If this loop is preceding another in the playlist, show that
                                        // by a "link icon" to the next one.
                                        LinkIndicator {
                                            visible: loop_rect.mapped_item.outgoing_edge != null
                                            height: loop_rect.height / 2
                                            width: height
                                            side: 'right'
                                            color: loop_rect.mapped_item.outgoing_edge_color
                                            loop_elem: loop_rect.mapped_item
                                            other_loop_elem: loop_rect.mapped_item.outgoing_edge

                                            anchors {
                                                right: parent.right
                                                verticalCenter: parent.verticalCenter
                                            }
                                        }

                                        // Same for incoming connections
                                        LinkIndicator {
                                            visible: loop_rect.mapped_item.incoming_edge != null
                                            height: loop_rect.height / 2
                                            width: height
                                            side: 'left'
                                            color: loop_rect.mapped_item.incoming_edge_color
                                            loop_elem: loop_rect.mapped_item
                                            other_loop_elem: loop_rect.mapped_item.incoming_edge

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
                                                add_connected_elem_and_push(mapped_item, new_elem, false, 0)
                                            }

                                            Rectangle {
                                                anchors.fill: parent
                                                opacity: 0.3
                                                visible: parent.containsDrag
                                            }
                                        }

                                        // DropArea for connecting a loop to be sequenced before this one
                                        DropArea {
                                            keys: ["LoopWidget"]
                                            anchors {
                                                left: parent.left
                                                right: parent.horizontalCenter
                                                top: parent.top
                                                bottom: parent.bottom
                                            }

                                            onDropped: (event) => {
                                                let src_loop_widget = drag.source
                                                let new_elem = playlist_element_factory.createObject(root, {
                                                    loop_widget: src_loop_widget,
                                                    loop_id: src_loop_widget.obj_id
                                                })
                                                add_connected_elem_and_push(mapped_item, new_elem, true, 0)
                                            }

                                            Rectangle {
                                                anchors.fill: parent
                                                opacity: 0.3
                                                visible: parent.containsDrag
                                            }
                                        }

                                        MouseArea {
                                            x: 0
                                            y: 0
                                            height: parent.height
                                            width: parent.width - right_drag_area.width
                                            acceptedButtons: Qt.RightButton
                                            onClicked: loop_menu.popup()
                                        }

                                        // Context menu
                                        Menu {
                                            id: loop_menu

                                            ShoopMenuItem {
                                                text: "Remove"
                                                onClicked: root.delete_elem_and_push(loop_rect.mapped_item)
                                            }
                                            ShoopMenuItem {
                                                text: "Unlink -->"
                                                shown: loop_rect.mapped_item.outgoing_edge ? true : false
                                                onClicked: root.unlink_and_push(loop_rect.mapped_item, loop_rect.mapped_item.outgoing_edge)
                                            }
                                            ShoopMenuItem {
                                                text: "<-- Unlink"
                                                shown: loop_rect.mapped_item.incoming_edge ? true : false
                                                onClicked: root.unlink_and_push(loop_rect.mapped_item.incoming_edge, loop_rect.mapped_item)
                                            }
                                            ShoopMenuItem {
                                                text: "Remove forced length"
                                                shown: loop_rect.mapped_item.maybe_forced_n_cycles ? true : false
                                                onClicked: root.force_elem_n_cycles_and_push(loop_rect.mapped_item, undefined)
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }}
                }
            }

            Rectangle {
                id: play_head
                width: 3
                height: show_track.height
                color: 'green'
                visible: parseInt(at) != 0

                readonly property int at: (composite_loop.position * root.cycle_width) / root.cycle_length
                x: at - root.x_offset
            }
        }
    }

    // Indicator shown on the sides of scheduled loop elements
    // to show they are linked to other loops sequentially.
    component LinkIndicator : Item {
        id: indicator

        // antialiasing
        layer.samples: 8
        layer.enabled: true

        property color color : 'blue'
        property string side : 'left' // or 'right'

        property var loop_elem
        property var other_loop_elem

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
                fillColor: indicator.color
                strokeWidth: -1

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