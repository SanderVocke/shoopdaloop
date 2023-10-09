import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3
import QtQuick.Dialogs
import ShoopDaLoop.PythonLogger
import ShoopDaLoop.PythonControlHandler
import ShoopDaLoop.PythonControlInterface

import "../generate_session.js" as GenerateSession
import "../generated/types.js" as Types

AppRegistries {
    id: root
    objectName: 'session'

    readonly property PythonLogger logger : PythonLogger { name: "Frontend.Qml.Session" }

    // The descriptor is an object matching the ShoopDaLoop session JSON
    // schema. The Session object will manage an actual session (consisting)
    // of loops, tracks, etc.) to match the loaded descriptor.
    // The descriptor may not be directly modified and is only used at initialization.
    // The actual descriptor can be retrieved with actual_session_descriptor().
    property var initial_descriptor : GenerateSession.generate_session(app_metadata.version_string, [], [], [], [])
    property var backend_type : global_args.backend_type
    property var backend_argstring : global_args.backend_argstring

    function actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to) {
        return GenerateSession.generate_session(
            app_metadata.version_string,
            tracks_widget.actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to),
            [],
            scenes_widget.actual_scene_descriptors,
            [],
            fx_chain_states_registry.all_values()
        );
    }

    readonly property string object_schema : 'session.1'
    SchemaCheck {
        descriptor: root.initial_descriptor
        schema: root.object_schema
        id: validator
    }

    readonly property bool saving : state_registry.n_saving_actions_active > 0
    readonly property bool loading : state_registry.n_loading_actions_active > 0
    readonly property bool doing_io : saving || loading
    readonly property var backend : session_backend
    property alias control_interface: control_interface
    property alias control_handler: control_interface

    Popup {
        visible: saving
        modal: true
        anchors.centerIn: parent
        parent: Overlay.overlay
        Text {
            color: Material.foreground
            text: "Saving..."
        }
    }
    Popup {
        visible: loading
        modal: true
        anchors.centerIn: parent
        parent: Overlay.overlay
        Text {
            color: Material.foreground
            text: "Loading..."
        }
    }

    // For (test) access
    property alias tracks: tracks_widget.tracks
    property bool loaded : tracks_widget.loaded

    TasksFactory { id: tasks_factory }

    function save_session(filename) {
        state_registry.reset_saving_loading()
        state_registry.save_action_started()
        var tempdir = file_io.create_temporary_folder()
        var tasks = tasks_factory.create_tasks_obj(root)
        var session_filename = tempdir + '/session.json'

        // TODO make this step asynchronous
        var descriptor = actual_session_descriptor(true, tempdir, tasks)
        file_io.write_file(session_filename, JSON.stringify(descriptor, null, 2))

        tasks.when_finished(() => {
            try {
                // TODO make this step asynchronous
                file_io.make_tarfile(filename, tempdir, false)
                root.logger.info("Session written to: " + filename)
            } finally {
                state_registry.save_action_finished()
                file_io.delete_recursive(tempdir)
                tasks.parent = null
                tasks.deleteLater()
            }
        })
    }

    function reload() {
        state_registry.clear([
            'sync_active',
            'scenes_widget'
        ])
        objects_registry.clear()
        tracks_widget.reload()
    }

    function queue_load_tasks(data_files_directory, add_tasks_to) {
        tracks_widget.queue_load_tasks(data_files_directory, add_tasks_to)
    }

    function get_track_control_widget(idx) {
        return tracks_widget.get_track_control_widget(idx)
    }

    function load_session(filename) {
        state_registry.reset_saving_loading()
        state_registry.load_action_started()
        var tempdir = file_io.create_temporary_folder()

        try {
            var tasks = tasks_factory.create_tasks_obj(root)

            file_io.extract_tarfile(filename, tempdir)
            root.logger.debug(`Extracted files: ${JSON.stringify(file_io.glob(tempdir + '/*', true), null, 2)}`)

            var session_filename = tempdir + '/session.json'
            var session_file_contents = file_io.read_file(session_filename)
            var descriptor = JSON.parse(session_file_contents)

            schema_validator.validate_schema(descriptor, validator.schema)
            root.initial_descriptor = descriptor
            root.logger.debug("Reloading session")
            reload()
            state_registry.load_action_started()

            let finish_fn = () => {
                root.logger.debug("Queueing load tasks")
                queue_load_tasks(tempdir, tasks)

                tasks.when_finished(() => {
                    try {
                        file_io.delete_recursive(tempdir)
                    } finally {
                        state_registry.load_action_finished()
                        tasks.parent = null
                        tasks.deleteLater()
                    }
                })
            }

            function connectOnce(sig, slot) {
                var f = function() {
                    slot.apply(this, arguments)
                    sig.disconnect(f)
                }
                sig.connect(f)
            }

            if(root.loaded) { finish_fn() }
            else {
                connectOnce(root.loadedChanged, finish_fn)
            }
        } catch(e) {
            file_io.delete_recursive(tempdir)
            throw e;
        }
    }

    RegistryLookup {
        id: selected_loops_lookup
        registry: state_registry
        key: 'selected_loop_ids'
    }
    property alias selected_loop_ids : selected_loops_lookup.object
    property list<var> selected_loops : selected_loop_ids ? Array.from(selected_loop_ids).map((id) => objects_registry.get(id)) : []

    RegistryLookup {
        id: targeted_loop_lookup
        registry: state_registry
        key: 'targeted_loop'
    }
    property alias targeted_loop : targeted_loop_lookup.object

    PythonControlInterface {
        id: control_interface
        qml_instance: this
        property bool ready: false

        Component.onCompleted: {
            scripting_engine.use_context(null)
            scripting_engine.create_lua_qobject_interface_in_current_context('shoop', control_interface)
            ready = true
        }

        property PythonLogger logger : PythonLogger { name: "Frontend.Session.ControlInterface" }

        property list<var> selected_loop_idxs : root.selected_loops ? root.selected_loops.map((l) => [l.track_idx, l.idx_in_track]) : []
        property var targeted_loop_idx: root.targeted_loop ? [root.targeted_loop.track_idx, root.targeted_loop.idx_in_track] : null

        function select_loops(loop_selector) {
            var rval = []
            if (loop_selector.length == 0) {
                rval = []
            } else if (Array.isArray(loop_selector)) {
                if (loop_selector.length == 0) { rval = [] }
                else {
                    if (Array.isArray(loop_selector[0])) {
                        // form [[x, y], [x, y], ...]
                        rval = loop_selector.map((coords) => {
                            if (coords[0] >= 0 && coords[0] < tracks_widget.tracks.length &&
                                coords[1] >= 0 && coords[1] < tracks_widget.tracks[coords[0]].loops.length) {
                                return tracks_widget.tracks[coords[0]].loops[coords[1]];
                            } else {        
                                return null
                            }
                        })
                    } else {
                        // form [x, y]
                        rval = [ tracks_widget.tracks[loop_selector[0]].loops[loop_selector[1]] ]
                    }
                }
            } else {
                // Form callback:  (loop) => true/false
                for(var t=0; t<tracks_widget.tracks.length; t++) {
                    let track = tracks_widget.tracks[t]
                    for(var l=0; l<track.loops.length; l++) {
                        let loop = track.loops[l]
                        if(loop_selector(loop)) { rval.push(loop) }
                    }
                }
            }
            logger.debug(`Selected loops for selector ${JSON.stringify(loop_selector)}: ${JSON.stringify(rval.map(l => l ? l.obj_id : null))}.`)
            return rval
        }

        function select_single_loop(loop_selector) {
            var handlers = select_loops(loop_selector)
            if (handlers.length != 1) {
                logger.throw_error('Handling loop call: multiple loops yielded while only one expected')
            }
            return handlers[0]
        }

        function loop_count_override(loop_selector) { return select_loops(loop_selector).filter(l => l != null).length }
        function loop_get_which_selected_override() { return selected_loop_idxs }
        function loop_get_which_targeted_override() { return targeted_loop_idx }
        function loop_get_mode_override(loop_selector) {
            return select_loops(loop_selector).map((l) => l.mode)
        }
        function loop_transition_override(loop_selector, mode, cycles_delay) {
            select_loops(loop_selector).forEach((h) => { h.transition(mode, cycles_delay, state_registry.get('sync_active')) } )
        }
        function loop_get_volume_override(loop_selector) { return select_single_loop(loop_selector).loop_get_volume(loop_selector) }
        function loop_get_balance_override(loop_selector) { return select_single_loop(loop_selector).loop_get_balance(loop_selector) }
        function loop_stop_override(loop_selector, cycles_delay, wait_sync) { select_loops(loop_selector).forEach((h) => { h.loop_stop(loop_selector, cycles_delay, wait_sync) } )}
        function loop_record_n_override(loop_selector, n, cycles_delay) { select_loops(loop_selector).forEach((h) => { h.record_n(cycles_delay, n) } )}
        function loop_record_with_targeted_override(loop_selector) {
            if (targeted_loop_idx) {
                select_loops(loop_selector).forEach((l) => l.record_with_targeted() )
            }
        }
        function loop_set_volume_override(loop_selector, volume) { select_loops(loop_selector).forEach((h) => { h.loop_set_volume(loop_selector, volume) } )}
        function loop_set_balance_override(loop_selector, balance) { select_loops(loop_selector).forEach((h) => { h.loop_set_balance(loop_selector, balance) } )}
        function loop_select_override(loop_selector, deselect_others) {
            var selection = new Set(select_loops(loop_selector).map((l) => l ? l.obj_id : null))
            selection.delete(null)
            if (!deselect_others && root.selected_loop_ids) {
                root.selected_loop_ids.forEach((id) => { selection.add(id) })
            }
            state_registry.replace('selected_loop_ids', selection)
        }
        function loop_target_override(loop_selector) {
            for(const loop of select_loops(loop_selector)) {
                if (loop) {
                    state_registry.replace('targeted_loop', loop)
                    return
                }
            }
            state_registry.replace('targeted_loop_id', null)
        }
        function loop_clear_override(loop_selector) {
            select_loops(loop_selector).forEach((h) => { h.clear() } )
        }
        function loop_untarget_all_override() { state_registry.replace('targeted_loop', null) }
        function loop_toggle_selected_override(loop_selector) { select_loops(loop_selector).forEach((h) => { h.loop_toggle_selected(loop_selector) } )}
        function loop_toggle_targeted_override(loop_selector) { select_loops(loop_selector).forEach((h) => { h.loop_toggle_targeted(loop_selector) } )}

        function select_ports(port_selector) {
            var rval = []
            if (Array.isArray(port_selector)) {
                if (port_selector.length == 0) { rval = [] }
                else {
                    // Form [track_idx, port_select_fn]
                    rval = tracks_widget.tracks[port_selector[0]].ports.filter((p) => port_selector[1](p))
                }
            } else {
                // Form [port_select_fn]
                tracks_widget.tracks.forEach((t) => {
                    rval = rval.concat(t.ports.filter((p) => port_selector(p)).map((p) => p.control_handler))
                })
            }
            logger.debug(`Selected ${rval.length} target port(s).`)
            return rval
        }

        function select_single_port(port_selector) {
            var handlers = select_loops(port_selector)
            if (handlers.length != 1) {
                logger.throw_error('Handling port call: multiple ports yielded while only one expected')
            }
            return handlers[0]
        }

        function port_get_volume_override(port_selector) { return select_ports(port_selector).port_get_volume(port_selector) }
        function port_get_muted_override(port_selector) { return select_ports(port_selector).port_get_muted(port_selector) }
        function port_get_input_muted_override(port_selector) { return select_ports(port_selector).port_get_input_muted(port_selector) }
        function port_mute_override(port_selector) { select_ports(port_selector).port_mute(port_selector) }
        function port_mute_input_override(port_selector) { select_ports(port_selector).port_mute_input(port_selector) }
        function port_unmute_override(port_selector) { select_ports(port_selector).port_unmute(port_selector) }
        function port_unmute_input_override(port_selector) { select_ports(port_selector).port_unmute_input(port_selector) }
        function port_set_volume_override(port_selector, vol) { select_ports(port_selector).port_set_volume(port_selector, vol) }
    }

    RegisterInRegistry {
        registry: root.state_registry
        key: 'control_interface'
        object: control_interface
    }

    LuaUserScript {
        script_name: 'keyboard.lua'
        script_code: control_interface.ready ? file_io.read_file(
            file_io.get_installation_directory() + '/lib/lua/keyboard.lua'
        ) : null
    }

    MouseArea {
        ExecuteNextCycle {
            id: takeFocus
            onExecute: {
                session_focus_item.forceActiveFocus()
            }
        }

        anchors.fill: parent
        focus: true
        id: session_focus_item

        //Keys.onLeftPressed: tracks_widget.navigate('left')
        //Keys.onRightPressed: tracks_widget.navigate('right')
        //Keys.onUpPressed: tracks_widget.navigate('up')
        //Keys.onDownPressed: tracks_widget.navigate('down')\

        Keys.onPressed: (event) => control_interface.key_pressed(event.key, event.modifiers)
        Keys.onReleased: (event) => control_interface.key_released(event.key, event.modifiers)

        property var focusItem : Window.activeFocusItem
        onFocusItemChanged: {
            root.logger.debug("Focus item changed: " + focusItem)
            if (!focusItem || focusItem == Window.contentItem) {
                takeFocus.trigger()
            }
        }

        onClicked: forceActiveFocus()
        Connections {
            target: release_focus_notifier
            function onFocusReleased() {
                session_focus_item.forceActiveFocus()
            }
        }
    }

    Backend {
        update_interval_ms: 30
        client_name_hint: 'ShoopDaLoop'
        backend_type: root.backend_type
        backend_argstring: root.backend_argstring
        id: session_backend

        anchors {
            fill: parent
            margins: 6
        }

        AppControls {
            id: app_controls

            backend: session_backend

            anchors {
                top: parent.top
                left: parent.left
                margins: 4
            }

            height: 40

            loading_session: root.loading
            saving_session: root.saving

            onLoadSession: (filename) => root.load_session(filename)
            onSaveSession: (filename) => root.save_session(filename)

            state_registry: root.state_registry
            objects_registry: root.objects_registry
        }

        ScenesWidget {
            id: scenes_widget

            initial_scene_descriptors: root.initial_descriptor.scenes
            objects_registry: root.objects_registry
            state_registry: root.state_registry
            
            width: 140
            anchors {
                top: app_controls.bottom
                left: parent.left
                bottom: logo_menu_area.top
                rightMargin: 6
            }
        }

        TracksWidget {
            id: tracks_widget

            anchors {
                top: app_controls.bottom
                left: scenes_widget.right
                bottom: parent.bottom
                right: parent.right
                bottomMargin: 4
                leftMargin: 4
            }

            initial_track_descriptors: root.initial_descriptor.tracks
            objects_registry: root.objects_registry
            state_registry: root.state_registry
        }

        Item {
            id: logo_menu_area

            anchors {
                bottom: parent.bottom
                right: tracks_widget.left
            }

            width: 160
            height: 160

            Item {
                width: childrenRect.width
                height: childrenRect.height

                anchors {
                    horizontalCenter: parent.horizontalCenter
                    top: parent.top
                    topMargin: 10
                }

                Image {
                    id: logo
                    anchors {
                        top: parent.top
                        topMargin: 6
                    }

                    height: 60
                    width: height / sourceSize.height * sourceSize.width
                    source: '../../resources/logo-small.png'
                    smooth: true
                }

                Text {
                    id: versiontext
                    anchors {
                        top: logo.bottom
                        horizontalCenter: logo.horizontalCenter
                        topMargin: 6
                    }
                    text: 'ShoopDaLoop v' + app_metadata.version_string
                    onLinkActivated: Qt.openUrlExternally(link)
                    color: Material.foreground
                    font.pixelSize: 12
                    linkColor: 'red'
                }
            }

            Grid {
                columns: 2
                spacing: 1
                horizontalItemAlignment: Grid.AlignHCenter
                verticalItemAlignment: Grid.AlignVCenter

                anchors {
                    horizontalCenter: parent.horizontalCenter
                    bottom: parent.bottom
                }

                Label {
                    id: dsptxt
                    text: "DSP:"
                }

                ProgressBar {
                    width: 80
                    from: 0.0
                    to: 100.0
                    value: session_backend.dsp_load
                }

                Label { 
                    text: "Xruns: " + session_backend.xruns.toString()
                }

                ExtendedButton {
                    tooltip: "Reset reported Xruns to 0."
                    id: reset_xruns
                    Label { 
                        text: "Reset"
                        anchors {
                            horizontalCenter: parent.horizontalCenter
                            verticalCenter: parent.verticalCenter
                        }
                    }
                    width: 40
                    height: 30
                    onClicked: session_backend.xruns = 0
                }
            }
        }
    }
}