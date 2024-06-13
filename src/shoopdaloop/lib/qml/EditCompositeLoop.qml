import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6
import QtQuick.Shapes 6.6

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

        ToolSeparator {
            anchors.top: parent.top
            anchors.bottom: parent.bottom
        }

        Label {
            anchors.verticalCenter: toolbar.verticalCenter
            text: "Kind:"
        }

        ShoopComboBox {
            id: kind_combo
            anchors.verticalCenter: toolbar.verticalCenter
            width: 150

            model: [
                'Regular',
                'Script'
            ]

            currentIndex: root.composite_loop.kind == 'regular' ? 0 : 1

            onActivated: (idx) => {
                if (model[idx] == "Regular") { confirm_to_regular_dialog.open() }
                else if (model[idx] == "Script") { root.to_script() }
            }

            Dialog {
                id: confirm_to_regular_dialog
                title: "Warning"
                standardButtons: Dialog.Yes | Dialog.Cancel
                onAccepted: root.to_regular()
                onRejected: kind_combo.currentIndex = 1
                modal: true

                parent: Overlay.overlay
                x: (parent.width-width) / 2
                y: (parent.height-height) / 2

                Text {
                    text: "Converting from script to regular will discard all modes and sections."
                    color: Material.foreground
                }
            }
        }

        ToolbarButton {
            anchors {
                verticalCenter: parent.verticalCenter
            }
            text: '?'
            size: toolbar.tool_buttons_size

            onClicked: info_popup.open()

            Popup {
                parent: Overlay.overlay
                x: (parent.width-width) / 2
                y: (parent.height-height) / 2
                modal: true
                width: 500
                height: 150
                id: info_popup
                closePolicy: Popup.CloseOnPressOutside | Popup.CloseOnEscape
                focus: true
                Label {
                    anchors.fill: parent
                    text: 'A regular composition will trigger its children with the same mode it was triggered with.\n' +
                          'It is useful for combining loops into bigger loops.\n' +
                          'A script composition specifies a specific mode to trigger each child with.\n' +
                          'It can only be triggered one way. It is useful for scheduling advanced sequences of e.g.\n' +
                          'combined recording and playback.'
                    horizontalAlignment: Text.AlignLeft
                    verticalAlignment: Text.AlignVCenter
                }
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
                            incoming_edges: [],
                            outgoing_edges: [],
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
        property var incoming_edges // refer to other PlaylistElements
        property var outgoing_edges // refer to other PlaylistElements
        property color incoming_edges_color
        property color outgoing_edges_color
        property var maybe_mode
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
            var prev_elems = []
            for(var j=0; j<_scheduled_playlists[i].length; j++) {
                let parallel_elems = []
                for(var h=0; h<_scheduled_playlists[i][j].length; h++) {
                    let ori = _scheduled_playlists[i][j][h]
                    let elem = {
                        "ori_elem": ori,
                        "swimlane": -1,
                        "incoming_edges": [],
                        "outgoing_edges": [],
                        "incoming_edges_color": 'grey',
                        "outgoing_edges_color": 'grey'
                    }
                    parallel_elems.push(elem)
                }
                if (prev_elems.length > 0) {
                    let c = pick_color()
                    prev_elems.forEach(p => parallel_elems.forEach(pp => {
                        p.outgoing_edges.push(pp)
                        pp.incoming_edges.push(p)
                        p.outgoing_edges_color = c
                        pp.incoming_edges_color = c
                    }))
                }
                playlist.push(parallel_elems)
                prev_elems = playlist[playlist.length - 1]
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
                let parallel_elems = playlist[j]
                for (var h=0; h<parallel_elems.length; h++) {
                    let elem = parallel_elems[h]
                    var swimlane = -1
                    let loop_widget = elem.ori_elem.loop_widget
                    let track_idx = loop_widget.track_idx
                    let swimlanes = swimlanes_per_track[track_idx]

                    // Check the existing swimlanes first
                    var check_swimlanes = (new Array(swimlanes.length).fill(0)).map((v, idx) => idx) // in ascending order
                    // If there are preceding elements in the same track(s), prefer to go in those swimlanes.
                    let preceding_swimlanes = elem.incoming_edges.filter(e => e.ori_elem.loop_widget.track_idx == loop_widget.track_idx).map(e => e.swimlane)
                    check_swimlanes = preceding_swimlanes.concat(check_swimlanes)
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
                    for(var k=elem.ori_elem.start_iteration; k<elem.ori_elem.end_iteration; k++) {
                        swimlanes[swimlane][k] = true
                    }

                    elem.swimlane = swimlane
                }
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
                var parallel_elems = []
                for(var h=0; h<playlist_elem_placeholders[i][j].length; h++) {
                    let info = playlist_elem_placeholders[i][j][h]
                    parallel_elems.push(playlist_element_factory.createObject(root, {
                        loop_widget: info.ori_elem.loop_widget,
                        loop_id: info.ori_elem.loop_id,
                        start_iteration: info.ori_elem.start_iteration,
                        end_iteration: info.ori_elem.end_iteration,
                        delay: info.ori_elem.delay,
                        swimlane: info.swimlane,
                        incoming_edges: j > 0 ? playlist[playlist.length-1] : [], // PlaylistElement previously created
                        outgoing_edges: [], // fill in later
                        incoming_edges_color: info.incoming_edges_color,
                        outgoing_edges_color: info.outgoing_edges_color,
                        maybe_forced_n_cycles: info.ori_elem.forced_n_cycles,
                        maybe_mode: info.ori_elem.mode
                    }))
                }
                playlist.push(parallel_elems)
            }
            rval.push(playlist)
        }
        return rval
    }
    // To prevent binding loops, we resolve some of the edges to other PlaylistElements
    // after initial creation.
    onPlaylist_elemsChanged: {
        playlist_elems.forEach(p => p.forEach(pp => pp.forEach(e => {
            e.incoming_edges.forEach(i => {
                i.outgoing_edges.push(e)
            })
        })))
    }

    // Flatten the elements-based playlists into a list of all elements.
    readonly property var flat_playlist_elems : {
        var rval = []
        for(var i=0; i<playlist_elems.length; i++) {
            for(var j=0; j<playlist_elems[i].length; j++) {
                for(var h=0; h<playlist_elems[i][j].length; h++) {
                    rval.push(playlist_elems[i][j][h])
                }
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
                'n_cycles': elem.maybe_forced_n_cycles ? elem.maybe_forced_n_cycles : undefined,
                'mode': (elem.maybe_mode !== null && elem.maybe_mode !== undefined) ? elem.maybe_mode : undefined
            }
        }
        var playlists = []
        for(var i=0; i<elems_playlists.length; i++) {
            let playlist = []
            for(var j=0; j<elems_playlists[i].length; j++) {
                let parallel_elems = []
                for(var h=0; h<elems_playlists[i][j].length; h++) {
                    parallel_elems.push(item_from_elem(elems_playlists[i][j][h]))
                }
                playlist.push(parallel_elems)
            }
            playlists.push(playlist)
        }
        composite_loop.playlists_in = playlists
    }

    // Delete PlaylistElement from the schedule and push
    function delete_elem_and_push(elem) {
        playlist_elems.forEach((p, idx) => {
            p.forEach((pp, pidx) => {
                function block_length(elems) {
                    return elems.length > 0 ?
                        Math.max(elems.map(e => e.delay + e.end_iteration - e.start_iteration)) : 0
                }

                if (pp.includes(elem)) {
                    let length_before = block_length(pp)
                    let index = pp.findIndex(e => e == elem)
                    pp.splice(index, 1)
                    let length_after = block_length(pp)

                    if (p.length > (pidx + 1)) {
                        p[pidx + 1].forEach(e => e.delay += (length_before - length_after))
                    }
                }
            })
        })
        let new_elems_schedule = playlist_elems.map(p => p.filter(e => e.length > 0)) // delete empty sections
        push_playlists(new_elems_schedule)
    }

    // Insert an element into an existing playlist
    function add_connected_elems_and_push(existing_elem, new_elems, put_before, delay) {
        function add_connected_elem(input_playlist, existing_elem, new_elem, put_before, delay) {
        // First, we have to find the existing element to access the set of parallel elements
        // containing it.
        let new_playlist_elems = input_playlist.map(p => p.map(pp => pp.map(e => e)))
        var existing_parallel_elems = null
        var existing_playlist = null
        new_playlist_elems.forEach(p => p.forEach(pp => { if(pp.includes(existing_elem)) { existing_parallel_elems = pp; existing_playlist = p } }))

        if(!existing_parallel_elems || !existing_playlist) {
            root.logger.warning(() => "Could not find playlist entry point for new element, ignoring")
            return;
        }

        // Now find the parallel set where our new element should be added.
        // If none exists (we are at the beginning/end of the playlist), create one.
        var new_parallel_elems = null
        if (put_before) {
            var prev_elems = null
            if (existing_elem.incoming_edges.length > 0) {
                let edge = existing_elem.incoming_edges[0]
                new_playlist_elems.forEach(p => p.forEach(pp => { if(pp.includes(edge)) { prev_elems = pp } }))
            }
            if (prev_elems === null) {
                existing_playlist.splice(0, 0, [])
                prev_elems = existing_playlist[0]
            }
            prev_elems.push(new_elem)
            existing_parallel_elems.forEach(e => {
                new_elem.outgoing_edges.push(e)
                e.incoming_edges.push(new_elem)
            })
        } else {
            var next_elems = null
            if (existing_elem.outgoing_edges.length > 0) {
                let edge = existing_elem.outgoing_edges[0]
                new_playlist_elems.forEach(p => p.forEach(pp => { if(pp.includes(edge)) { next_elems = pp } }))
            }
            if (next_elems === null) {
                existing_playlist.push([])
                next_elems = existing_playlist[existing_playlist.length - 1]
            }
            next_elems.push(new_elem)
            existing_parallel_elems.forEach(e => {
                new_elem.incoming_edges.push(e)
                e.outgoing_edges.push(new_elem)
            })
        }
        new_elem.delay = delay
        var new_elems_schedule = []
        for (var i=0; i<new_playlist_elems.length; i++) {
            let playlist = new_playlist_elems[i]
            let new_playlist = []
            for (var j=0; j<playlist.length; j++) {
                let parallel_elems = []
                for(var h=0; h<playlist[j].length; h++) {
                    let elem = playlist[j][h]
                    parallel_elems.push(elem)
                }
                new_playlist.push(parallel_elems)
            }
            new_elems_schedule.push(new_playlist)
        }
        return new_elems_schedule
        }

        var under_construction = playlist_elems
        if (put_before) {
            for(var i=0; i<new_elems.length; i++) {
                under_construction = add_connected_elem(under_construction, existing_elem, new_elems[i], put_before, delay)
            }
        } else {
            var add_after = existing_elem
            for(var i=new_elems.length-1; i>=0; i--) {
                under_construction = add_connected_elem(under_construction, add_after, new_elems[i], put_before, delay)
                add_after = new_elems[i]
            }
        }

        push_playlists(under_construction)
    }

    // Duplicate an element N times sequentially and push
    function duplicate_elem_and_push(elem, n_copies) {
        let copies = []
        for (var i=0; i<n_copies; i++) {
            let copy = copy_elem(elem)
            copies.push(copy)
        }
        add_connected_elems_and_push(elem, copies, false, 0)
    }

    // Create a new playlist with a single element in it.
    function add_new_playlist_with_elem_and_push(new_elem, delay) {
        new_elem.start_iteration = delay
        new_elem.end_iteration = new_elem.start_iteration + new_elem.loop_widget.n_cycles
        var new_elems_schedule = Array.from(playlist_elems)
        new_elems_schedule.push([[new_elem]])
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
            if (!p.find(pp => pp.includes(from_elem)) || !p.find(pp => pp.includes(to_elem))) { new_elems_schedule.push(Array.from(p)); return; }
            let first_playlist = []
            let second_playlist = []
            var found = false
            for(var i=0; i<p.length; i++) {
                if (found) {
                    second_playlist.push(p[i])
                    if (p[i].includes(to_elem)) {
                        p[i].forEach(pp => {
                            pp.delay = pp.start_iteration
                            pp.incoming_edges = []
                        })
                    }
                } else if (p[i].includes(from_elem)) {
                    found = true
                    first_playlist.push(p[i])
                    p[i].forEach(pp => { pp.outgoing_edges = [] })
                } else {
                    first_playlist.push(p[i])
                }
            }
            new_elems_schedule.push(Array.from(first_playlist));
            new_elems_schedule.push(Array.from(second_playlist));
        })
        push_playlists(new_elems_schedule)
    }

    function change_mode_and_push(elem, mode) {
        elem.maybe_mode = mode
        push_playlists(playlist_elems)
    }
    
    // Convert to a regular composite schedule (no modes)
    function to_regular() {
        playlist_elems.forEach(p => p.forEach(pp => pp.forEach(e => e.maybe_mode = null)))
        push_playlists(playlist_elems)
    }

    // Convert to a script composite schedule (all modes specified)
    function to_script() {
        playlist_elems.forEach(p => p.forEach(pp => pp.forEach(e => {
            if (e.maybe_mode === null) {
                e.maybe_mode = ShoopConstants.LoopMode.Playing
            }
        })))
        push_playlists(playlist_elems)
    }

    function copy_elem(elem) {
        let rval = playlist_element_factory.createObject(root, {
            loop_widget: elem.loop_widget,
            loop_id: elem.loop_id,
            start_iteration: elem.start_iteration,
            end_iteration: elem.end_iteration,
            delay: elem.delay,
            swimlane: elem.swimlane,
            incoming_edges: elem.incoming_edges,
            outgoing_edges: elem.outgoing_edges,
            incoming_edges_color: elem.incoming_edges_color,
            outgoing_edges_color: elem.outgoing_edges_color,
            maybe_forced_n_cycles: elem.maybe_forced_n_cycles,
            maybe_mode: elem.maybe_mode
        })
        return rval
    }

    component Track : Item {
        id: track_root

        property var track
        readonly property int track_idx : track.track_idx

        property int x_offset : 0
        
        height: childrenRect.height

        clip: true

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
                top: parent ? parent.top : undefined
                left: parent ? parent.left : undefined
                right: parent ? parent.right : undefined
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
                        width: content_rect.width

                        Repeater {
                            id: swimlanes
                            model: track_root.n_swimlanes

                            // background rectangle for the entire swimlane.
                            Rectangle {
                                id: swimlane
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

                                        color: {
                                            switch(mapped_item.loop_widget.mode) {
                                            case ShoopConstants.LoopMode.Playing:
                                                return '#004400';
                                            case ShoopConstants.LoopMode.PlayingDryThroughWet:
                                                return '#333300';
                                            case ShoopConstants.LoopMode.Recording:
                                                return '#660000';
                                            case ShoopConstants.LoopMode.RecordingDryIntoWet:
                                                return '#663300';
                                            default:
                                                return '#000044';
                                            }
                                        }
                                        border.color: 'grey'
                                        border.width: 2

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

                                        // Draggy rect for right-side width ajustment or duplication.
                                        // For changing width it is just on the right-hand side of the screen.
                                        // When Control is pressed, the drag area covers the whole loop and is
                                        // used to duplicate it.
                                        Rectangle {
                                            id: right_width_adjuster
                                            anchors {
                                                right: parent.right
                                                top: parent.top
                                                bottom: parent.bottom
                                            }
                                            width: key_modifiers.control_pressed ? parent.width : 20

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
                                                cursorShape: key_modifiers.control_pressed ? Qt.DragCopyCursor : Qt.SizeHorCursor

                                                onReleased: {
                                                    if (drag.active) {
                                                        let new_n = right_resize_preview.dragged_cycle
                                                        if (new_n >= 1) {
                                                            if (key_modifiers.control_pressed) {
                                                                // Duplicate loop
                                                                let length = mapped_item.end_iteration - mapped_item.start_iteration
                                                                let n_copies = Math.ceil((new_n - length) / length)
                                                                if (n_copies > 0) {
                                                                    duplicate_elem_and_push(mapped_item, n_copies)
                                                                }
                                                            } else {
                                                                // Resize loop
                                                                force_elem_n_cycles_and_push(loop_rect.mapped_item, new_n)
                                                            }
                                                        }
                                                    }
                                                }

                                                drag {
                                                    axis: "XAxis"
                                                    target: right_width_adjuster_movable
                                                }
                                            }
                                        }

                                        MaterialDesignIcon {
                                            size: 20
                                            anchors {
                                                verticalCenter: parent.verticalCenter
                                                right: right_side_link_indicator.left
                                            }
                                            name: 'alert-box'
                                            color: Material.foreground
                                            visible: mapped_item.maybe_forced_n_cycles ? true : false
                                        }

                                        ToolbarButton {
                                            id: mode_button
                                            visible: root.composite_loop.kind == 'script'
                                            anchors {
                                                left: left_side_link_indicator.right
                                                verticalCenter: parent.verticalCenter
                                            }

                                            size: 20

                                            readonly property var model: [
                                                { mode: ShoopConstants.LoopMode.Playing, icon: 'play', color: 'green' },
                                                { mode: ShoopConstants.LoopMode.Recording, icon: 'record', color: 'red' },
                                                { mode: ShoopConstants.LoopMode.PlayingDryThroughWet, icon: 'play', color: 'orange' },
                                                { mode: ShoopConstants.LoopMode.RecordingDryIntoWet, icon: 'record', color: 'orange' }
                                            ]
                                            property var cur_idx: {
                                                let m = loop_rect.mapped_item.maybe_mode
                                                let idx = model.findIndex(e => e.mode == m)
                                                return idx < 0 ? null : idx
                                            }
                                            readonly property var maybe_mode: (cur_idx === null) ? null : model[cur_idx].mode

                                            onMaybe_modeChanged: {
                                                if (maybe_mode !== null && maybe_mode !== loop_rect.mapped_item.maybe_mode) {
                                                    root.change_mode_and_push(loop_rect.mapped_item, maybe_mode)
                                                }
                                            }

                                            MaterialDesignIcon {
                                                anchors.centerIn: parent
                                                size: 20
                                                name: (parent.cur_idx === null || parent.cur_idx === undefined) ? 'play' : parent.model[parent.cur_idx].icon
                                                color: (parent.cur_idx === null || parent.cur_idx === undefined) ? 'grey' : parent.model[parent.cur_idx].color
                                            }

                                            onClicked: mode_menu.popup()

                                            Menu {
                                                id: mode_menu
                                                
                                                Repeater {
                                                    model: mode_button.model

                                                    ShoopMenuItem {
                                                        height: 20
                                                        width: 30
                                                        onClicked: {
                                                            mode_button.cur_idx = index
                                                        }

                                                        MaterialDesignIcon {
                                                            size: 20
                                                            name: mode_button.model[index].icon
                                                            color: mode_button.model[index].color
                                                        }
                                                    }
                                                }
                                            }
                                        }

                                        // If this loop is preceding another in the playlist, show that
                                        // by a "link icon" to the next one.
                                        LinkIndicator {
                                            id: right_side_link_indicator
                                            visible: loop_rect.mapped_item.outgoing_edges.length > 0
                                            height: loop_rect.height / 2
                                            width: height
                                            side: 'right'
                                            color: loop_rect.mapped_item.outgoing_edges_color
                                            loop_elem: loop_rect.mapped_item
                                            other_loop_elem: loop_rect.mapped_item.outgoing_edges

                                            anchors {
                                                right: parent.right
                                                verticalCenter: parent.verticalCenter
                                            }
                                        }

                                        // Same for incoming connections
                                        LinkIndicator {
                                            id: left_side_link_indicator
                                            visible: loop_rect.mapped_item.incoming_edges.length > 0
                                            height: loop_rect.height / 2
                                            width: height
                                            side: 'left'
                                            color: loop_rect.mapped_item.incoming_edges_color
                                            loop_elem: loop_rect.mapped_item
                                            other_loop_elem: loop_rect.mapped_item.incoming_edges

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
                                                    loop_id: src_loop_widget.obj_id,
                                                    incoming_edges: [],
                                                    outgoing_edges: []
                                                })
                                                add_connected_elems_and_push(mapped_item, [new_elem], false, 0)
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
                                                    loop_id: src_loop_widget.obj_id,
                                                    incoming_edges: [],
                                                    outgoing_edges: []
                                                })
                                                add_connected_elems_and_push(mapped_item, [new_elem], true, 0)
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
                                                shown: loop_rect.mapped_item.outgoing_edges.length > 0 ? true : false
                                                onClicked: root.unlink_and_push(loop_rect.mapped_item, loop_rect.mapped_item.outgoing_edges[0])
                                            }
                                            ShoopMenuItem {
                                                text: "<-- Unlink"
                                                shown: loop_rect.mapped_item.incoming_edges.length > 0 ? true : false
                                                onClicked: root.unlink_and_push(loop_rect.mapped_item.incoming_edges[0], loop_rect.mapped_item)
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