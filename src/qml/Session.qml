import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6
import QtQuick.Dialogs 6.6
import ShoopDaLoop.Rust
import "./js/generate_session.js" as GenerateSession

Item {
    id: root
    objectName: 'session'

    readonly property ShoopRustLogger logger : ShoopRustLogger { name: "Frontend.Qml.Session" }

    // The descriptor is an object matching the ShoopDaLoop session JSON
    // schema. The Session object will manage an actual session (consisting)
    // of loops, tracks, etc.) to match the loaded descriptor.
    // The descriptor may not be directly modified and is only used at initialization.
    // The actual descriptor can be retrieved with actual_session_descriptor().
    property var initial_descriptor : GenerateSession.generate_session(global_args.version_string, null, [], [], [], [])
    property var backend_type : global_args.backend_type
    property int backend_update_interval_ms : 30

    property alias driver_setting_overrides : session_backend.driver_setting_overrides

    ExecuteNextCycle {
        id: auto_session_loader
        property string filename
        property bool ignore_resample_warning: false
        interval: 10
        onExecute: {
            load_session(filename, ignore_resample_warning)
            ignore_resample_warning = false
        }
    }

    property bool did_auto_load: false
    onLoadedChanged: {
        root.logger.debug(`loaded -> ${loaded}`)
        if (loaded) {
            if (global_args.load_session_on_startup && !did_auto_load) {
                did_auto_load = true
                var filename = global_args.load_session_on_startup
                auto_session_loader.filename = filename
                root.logger.debug("Loading session on startup: " + filename)
                auto_session_loader.trigger()
            }
            if (global_args.test_grab_screens_dir) {
                test_grab_screens_and_quit(global_args.test_grab_screens_dir)
            }
            if (global_args.quit_after >= 0.0) {
                root.logger.info(`Auto-quit scheduled for ${global_args.quit_after} seconds.`)
                autoquit_timer.interval = global_args.quit_after * 1000.0
                autoquit_timer.start()
            }
        }
    }

    Timer {
        id: autoquit_timer
        interval: 100
        repeat: false
        running: false
        onTriggered: {
            root.logger.info("Auto-quitting as per request.")
            Qt.callLater(Qt.quit)
        }
    }

    ExecuteNextCycle {
        id: test_screen_grab_trigger
        property string output_folder
        onExecute: {
            ShoopRustTestScreenGrabber.grab_all(output_folder)
            root.logger.info("Screenshots written to: " + output_folder + ". Quitting.")
            Qt.callLater(Qt.quit)
        }
    }
    RegistrySelects {
        registry: AppRegistries.objects_registry
        select_fn: (obj) => obj && obj.object_schema && obj.object_schema.match(/loop.[0-9]+/)
        id: lookup_loops
        values_only: true
    }
    property alias loops : lookup_loops.objects
    function test_grab_screens_and_quit(output_folder) {
        // We are supposed to take screenshots of application windows, output them
        // and exit.
        test_screen_grab_trigger.output_folder = output_folder
        test_screen_grab_trigger.trigger()
    }

    property var tracks: [sync_track, ...tracks_widget.tracks]

    property bool settings_io_enabled: false

    function actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to) {
        let track_groups = [
            {
                'name': 'sync',
                'tracks': [ root.sync_track.actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to) ]
            },
            {
                'name': 'main',
                'tracks': tracks_widget.actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to)
            }
        ]

        return GenerateSession.generate_session(
            global_args.version_string,
            session_backend.sample_rate,
            track_groups,
            [],
            [],
            [],
            AppRegistries.fx_chain_states_registry.all_values()
        );
    }

    readonly property string object_schema : 'session.1'
    SchemaCheck {
        descriptor: root.initial_descriptor
        schema: root.object_schema
        id: validator
    }

    readonly property bool doing_io : AppRegistries.state_registry.io_active
    readonly property var backend : session_backend
    property alias control_interface: control_interface

    Popup {
        visible: doing_io
        modal: true
        anchors.centerIn: parent
        parent: Overlay.overlay
        Text {
            color: Material.foreground
            text: "I/O active..."
        }
    }

    // For (test) access
    property alias main_tracks: tracks_widget.tracks
    property alias sync_track: sync_loop_loader.track_widget
    property bool loaded : tracks_widget.loaded && sync_loop_loader.loaded
    function get_tracks_widget() { return tracks_widget }

    property var main_track_descriptors: {
        let groups = root.initial_descriptor.track_groups;
        for (var i=0; i<groups.length; i++) {
            if (groups[i].name == 'main') { return groups[i].tracks; }
        }
        return []
    }
    property var sync_loop_track_descriptor: {
        let groups = root.initial_descriptor.track_groups;
        for (var i=0; i<groups.length; i++) {
            if (groups[i].name == 'sync') {
                let _t = groups[i].tracks
                if (_t.length != 1) {
                    root.logger.error(`Found ${_t.length} tracks in sync group instead of 1.`)
                }
                if (_t.length > 0) {
                    let _l = _t[0].loops
                    if (_l.length != 1) {
                        root.logger.error(`Found ${_l.length} loops in sync track instead of 1.`)
                    }
                    if (_l.length > 0) {
                        return _t[0]
                    }
                }
            }
        }
        return null
    }

    Component {
        id: task_observer_factory
        TaskObserver {}
    }

    function create_task_observer() {
        if (task_observer_factory.status == Component.Error) {
            throw new Error("Session: Failed to load factory: " + task_observer_factory.errorString())
        } else if (task_observer_factory.status != Component.Ready) {
            throw new Error("Session: Factory not ready: " + task_observer_factory.status.toString())
        } else {
            return task_observer_factory.createObject(root, {})
        }
    }

    function save_session(filename) {
        root.logger.debug(`saving session to: ${filename}`)
        var tempdir = ShoopRustFileIO.create_temporary_folder()
        root.logger.trace(`Created temporary folder: ${tempdir}`)
        if (tempdir == null) {
            throw new Error("Failed to create temporary folder")
        }
        var session_filename = tempdir + '/session.json'
        AppRegistries.state_registry.set_active_io_task_fn(() => {
            AppRegistries.state_registry.set_force_io_active(true)

            var observer = create_task_observer()

            observer.finished.connect((success) => {
                if (success) {
                    try {
                        // TODO make this step asynchronous
                        if (!ShoopRustFileIO.make_tarfile(filename, tempdir)) {
                            throw new Error(`Failed to create tarfile ${filename}`)
                        }
                        root.logger.info("Session written to: " + filename)
                    } finally {
                        ShoopRustFileIO.delete_recursive(tempdir)
                    }
                } else {
                    root.logger.error("Writing session failed.")
                }
                AppRegistries.state_registry.set_force_io_active(false)
            })

            // TODO make this step asynchronous
            var descriptor = actual_session_descriptor(true, tempdir, observer)
            if(!ShoopRustFileIO.write_file(session_filename, JSON.stringify(descriptor, null, 2))) {
                throw new Error(`Failed to write session file ${session_filename}`)
            }
            observer.start()

            return observer
        })
    }

    function unload_session() {
        root.logger.debug("Unloading session")
        AppRegistries.state_registry.clear([
            'sync_active'
        ])
        AppRegistries.objects_registry.clear()
        tracks_widget.unload()
        sync_loop_loader.unload()
        root.logger.debug("Session unloaded")
    }

    function load_current_session() {
        root.logger.debug("Reloading session metadata")
        tracks_widget.load()
        sync_loop_loader.load()
    }

    function queue_load_tasks(data_files_directory, from_sample_rate, to_sample_rate, add_tasks_to) {
        root.logger.debug(`Queue load tasks for main tracks`)
        tracks_widget.queue_load_tasks(data_files_directory, from_sample_rate, to_sample_rate, add_tasks_to)
        if (sync_loop_loader.track_widget) {
            root.logger.debug(`Queue load tasks for sync track`)
            sync_loop_loader.track_widget.queue_load_tasks(data_files_directory, from_sample_rate, to_sample_rate, add_tasks_to)
        }
    }

    Dialog {
        x : (root.width - width) / 2
        y : (root.height - height) / 2
        id: confirm_sample_rate_convert_dialog
        property string session_filename : ""
        property int from : 0
        property int to : 0

        standardButtons: Dialog.Ok | Dialog.Cancel

        Text {
            color: Material.foreground
            text: `Loading this session will resample it from ${confirm_sample_rate_convert_dialog.from} to ${confirm_sample_rate_convert_dialog.to} samples/s.`
        }

        onAccepted: {
            close()
            auto_session_loader.filename = session_filename
            auto_session_loader.ignore_resample_warning = true
            root.logger.debug("Loading session on startup: " + filename)
            auto_session_loader.trigger()
        }
    }

    QtObject {
        id: is_loaded_as_task_interface
        property bool active: !root.loaded
        property bool success: true
    }

    function load_session(filename, ignore_resample_warning=false) {
        root.logger.debug(`loading session: ${filename}`)
        var tempdir = ShoopRustFileIO.create_temporary_folder()

        try {
            root.unload_session()

            ShoopRustFileIO.extract_tarfile(filename, tempdir)
            root.logger.debug(`Extracted files to ${tempdir}: ${JSON.stringify(ShoopRustFileIO.glob(tempdir + '/*'), null, 2)}`)

            var session_filename = tempdir + '/session.json'
            var session_file_contents = ShoopRustFileIO.read_file(session_filename)
            var descriptor = JSON.parse(session_file_contents)
            root.logger.trace(`Session descriptor: ${JSON.stringify(descriptor, null, 2)}`)
            let our_sample_rate = session_backend.sample_rate
            let incoming_sample_rate = descriptor.sample_rate

            if (!ShoopRustSchemaValidator.validate_schema(descriptor, "Session object", validator.schema, false)) {
                return;
            }

            if (our_sample_rate != incoming_sample_rate && !ignore_resample_warning) {
                confirm_sample_rate_convert_dialog.session_filename = filename
                confirm_sample_rate_convert_dialog.from = incoming_sample_rate
                confirm_sample_rate_convert_dialog.to = our_sample_rate
                confirm_sample_rate_convert_dialog.open()
                return;
            }

            if (our_sample_rate != incoming_sample_rate) {
                descriptor = GenerateSession.convert_session_descriptor_sample_rate(descriptor, incoming_sample_rate, our_sample_rate)
            }

            root.initial_descriptor = descriptor
            root.load_current_session()

            let finish_fn = () => {
                AppRegistries.state_registry.set_active_io_task_fn(() => {
                    var observer = create_task_observer()

                    root.logger.debug("Queueing load tasks")

                    observer.finished.connect((success) => {
                        if (success) {
                            try {
                                ShoopRustFileIO.delete_recursive(tempdir)
                            } finally {
                                root.logger.info("Session loaded from: " + filename)
                            }
                        } else {
                            root.logger.error("Loading session failed")
                        }

                    })

                    queue_load_tasks(tempdir, incoming_sample_rate, our_sample_rate, observer)

                    observer.start()
                    return observer
                })
                AppRegistries.state_registry.set_force_io_active(false)
            }

            function connectOnce(sig, slot) {
                var f = function() {
                    slot.apply(this, arguments)
                    sig.disconnect(f)
                }
                sig.connect(f)
            }

            AppRegistries.state_registry.set_force_io_active(true)
            if(root.loaded) { finish_fn() }
            else {
                connectOnce(root.loadedChanged, finish_fn)
            }
        } catch(e) {
            ShoopRustFileIO.delete_recursive(tempdir)
            throw e;
        }
    }

    SelectedLoops { id: selected_loops_lookup }
    property alias selected_loops: selected_loops_lookup.loops
    function select_loops(loops, clear=false) {
        var selection = new Set(loops.map(l => l.obj_id))
        selection.delete(null)
        if (!clear && session.selected_loops) {
            root.selected_loops.forEach((l) => { selection.add(l.obj_id) })
        }
        AppRegistries.state_registry.replace('selected_loop_ids', selection)
    }

    RegistryLookup {
        id: targeted_loop_lookup
        registry: AppRegistries.state_registry
        key: 'targeted_loop'
    }
    property alias targeted_loop : targeted_loop_lookup.object
    function target_loop(loop) {
        AppRegistries.state_registry.set_targeted_loop(loop)
    }

    RegisterInRegistry {
        registry: AppRegistries.state_registry
        key: 'control_interface'
        object: control_interface
    }

    LuaScriptManager {
        // Only connect to the control interface when it is ready
        control_interface: control_interface

        RegisterInRegistry {
            object: parent
            key: 'lua_script_manager'
            registry: AppRegistries.state_registry
        }
    }

    Loader {
        active: global_args.monkey_tester

        sourceComponent: MonkeyTester {
            session: root
            Component.onCompleted: {
                start()
            }
        }
    }

    signal key_pressed(var event)
    signal key_released(var event)

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

        Keys.onPressed: (event) => root.key_pressed(event)
        Keys.onReleased: (event) => root.key_released(event)

        property var focusItem : Window.activeFocusItem
        onFocusItemChanged: {
            root.logger.debug("Focus item changed: " + focusItem)
            if (!focusItem || focusItem == Window.contentItem) {
                takeFocus.trigger()
            }
        }

        onClicked: forceActiveFocus()
        Connections {
            target: ShoopRustReleaseFocusNotifier
            function onFocus_released() {
                session_focus_item.forceActiveFocus()
            }
        }
    }

    Backend {
        update_interval_ms: root.backend_update_interval_ms
        client_name_hint: 'ShoopDaLoop'
        backend_type: root.backend_type
        id: session_backend
        driver_setting_overrides: ({})

        onUpdated_on_gui_thread: {
            app_controls.add_dsp_load_point(dsp_load)
            app_controls.add_audio_buffer_pool_point(n_audio_buffers_created, n_audio_buffers_available)
            app_controls.add_backend_refresh_interval_point(last_update_interval)
        }

        SessionControlHandler {
            id: control_interface
            session: root
            backend: session_backend
        }

        ShoopRustMidiControlPort {
            backend: session_backend
            id: midi_control_port
            name_hint: "app-control"
            direction: ShoopRustConstants.PortDirection.Input

            RegistryLookup {
                id: lookup_autoconnect
                registry: AppRegistries.state_registry
                key: 'autoconnect_input_regexes'
            }

            autoconnect_regexes: lookup_autoconnect.object || []
            may_open: true

            onMsg_received: msg => midi_control.handle_midi(msg, midi_control_port)
        }

        MidiControl {
            id: midi_control
            control_interface: control_interface
            configuration: lookup_midi_configuration.object || fallback

            MidiControlConfiguration { id: fallback }

            RegistryLookup {
                registry: AppRegistries.state_registry
                key: 'midi_control_configuration'
                id: lookup_midi_configuration
            }
        }

        RegisterInRegistry {
            registry: AppRegistries.state_registry
            key: 'midi_control_port'
            object: midi_control_port
        }

        anchors {
            fill: parent
            margins: 6
        }

        AppControls {
            id: app_controls

            backend: session_backend
            settings_io_enabled: root.settings_io_enabled

            anchors {
                top: parent.top
                left: parent.left
                margins: 4
            }

            height: 40

            onLoadSession: (filename) => root.load_session(filename)
            onSaveSession: (filename) => root.save_session(filename)
            onProcessThreadSegfault: session_backend.segfault_on_process_thread()
            onProcessThreadAbort: session_backend.abort_on_process_thread()
            onOpenConnections: connections_window.visible = true
        }

        Item {
            id: welcome_text
            visible: root.main_tracks.length == 0
            anchors.centerIn: parent

            width: childrenRect.width
            height: childrenRect.height

            Label {
                text: {
                    var rval = 'To get started:'
                    rval += '\n\n- Set up content for the sync loop by:'
                    rval += '\n  - Recording mono content into it, or'
                    rval += '\n  - Loading audio (right-click on loop -> "Load Audio..."), or'
                    rval += '\n  - Generating a click loop (right-click on loop -> "Click Loop...").'
                    rval += '\n\n- Add a track by using the (+) at the top left.'
                    return rval
                }
            }
        }

        TracksWidget {
            id: tracks_widget

            backend : session_backend

            anchors {
                top: app_controls.bottom
                left: parent.left
                bottom: pane_area.visible ? pane_area.top : bottom_bar.top
                right: logo_menu_area.left
                bottomMargin: 4
                leftMargin: 4
            }

            initial_track_descriptors: root.main_track_descriptors

            ConnectionsWindow {
                id: connections_window

                function flatten(arr) {
                    return arr.reduce((acc, current) => {
                        return acc.concat(Array.isArray(current) ? flatten(current) : current);
                    }, []);
                }

                property var tracks: [sync_loop_loader.track_widget].concat(tracks_widget.tracks)

                audio_in_ports : flatten(tracks ? tracks.map(t => t ? t.audio_in_ports : []) : [])
                audio_out_ports : flatten(tracks ? tracks.map(t => t ? t.audio_out_ports : []) : [])
                audio_send_ports: flatten(tracks ? tracks.map(t => t ? t.audio_send_ports : []) : [])
                audio_return_ports: flatten(tracks ? tracks.map(t => t ? t.audio_return_ports : []) : [])
                midi_in_ports : flatten(tracks ? tracks.map(t => t ? t.midi_in_ports : []) : [])
                midi_out_ports : flatten(tracks ? tracks.map(t => t ? t.midi_out_ports : []) : [])
                midi_send_ports: flatten(tracks ? tracks.map(t => t ? t.midi_send_ports : []) : [])
            }
        }

        ResizeableItem {
            id: pane_area

            readonly property bool open : AppRegistries.state_registry.details_open
            visible: open

            property real active_height: 200

            onOpenChanged: {
                if (open) {
                    height = pane_area.active_height
                } else {
                    if (height > 0) {
                        pane_area.active_height = height
                    }
                }
            }

            height: open ? active_height : 0 // Initial value
            max_height: root.height - 50

            top_drag_enabled: true
            top_drag_area_y_offset: pane.pane_y_offset

            anchors {
                bottom: bottom_bar.top
                left: parent.left
                right: logo_menu_area.left
            }

            DetailsPane {
                id: pane
                temporary_items : Array.from(root.selected_loops).map(l => ({
                        'title': l.name + ' (selected)',
                        'item': l,
                        'autoselect': true
                    }))

                RegisterInRegistry {
                    registry: AppRegistries.state_registry
                    key: 'main_details_pane'
                    object: pane
                }

                anchors.fill: parent

                sync_track: root.sync_track
                main_tracks: root.main_tracks
            }
        }

        Item {
            id: bottom_bar
            height: details_toggle.height

            anchors {
                left: parent.left
                right: logo_menu_area.left
                bottom: parent.bottom
            }

            ToolbarButton {
                anchors {
                    left: parent.left
                    bottom: parent.bottom
                }

                id: details_toggle
                text: 'details'
                togglable: true
                height: 26

                Component.onCompleted: AppRegistries.state_registry.set_details_open(checked)
                onCheckedChanged: AppRegistries.state_registry.set_details_open(checked)
                Connections {
                    target: AppRegistries.state_registry
                    function onDetails_openChanged() { details_toggle.checked = AppRegistries.state_registry.details_open }
                }
            }
        }

        Rectangle {
            id: sync_loop_area
            color: "#555555"
            height: 120

            anchors {
                right: parent.right
                left: logo_menu_area.left
                bottom: logo_menu_area.top
                leftMargin: 10
                rightMargin: 10
            }

            Loader {
                active: false
                id: sync_loop_loader
                height: sync_loop_area.height

                anchors {
                    left: parent.left
                    right: parent.right
                }

                property bool loaded: false
                property var initial_descriptor : null

                Component.onCompleted: {
                    unload()
                    load()
                }

                function load() {
                    root.logger.debug("sync loop: loading session")
                    initial_descriptor = root.sync_loop_track_descriptor
                    active = Qt.binding(() => {
                        return initial_descriptor != null
                    })
                }

                function unload() {
                    root.logger.debug("sync loop: unloading session")
                    if (item && item.track_widget) { item.track_widget.unload() }
                    active = false
                    loaded = false
                    initial_descriptor = null
                }

                property var track_widget: item ?
                    item.track_widget : undefined

                sourceComponent: Item {

                    property alias track_widget: sync_loop_widget
                    property alias track_control_widget: sync_loop_control_widget

                    anchors {
                        fill: sync_loop_loader
                        leftMargin: 8
                        rightMargin: 8
                    }

                    TrackWidget {
                        id: sync_loop_widget

                        backend : session_backend

                        anchors {
                            left: parent.left
                            right: parent.right
                            top: parent.top
                        }

                        initial_descriptor: sync_loop_loader.initial_descriptor

                        onLoadedChanged: sync_loop_loader.loaded = loaded
                        name_editable: false
                        sync_loop_layout: true
                        track_idx : -1
                    }

                    TrackControlWidget {
                        id: sync_loop_control_widget

                        anchors {
                            left: parent.left
                            right: parent.right
                            bottom: parent.bottom
                            bottomMargin: 15
                        }

                        initial_track_descriptor: root.sync_loop_track_descriptor
                        track: sync_loop_widget
                    }
                }
            }
        }

        Item {
            id: logo_menu_area

            anchors {
                bottom: parent.bottom
                right: parent.right
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
                    source: "file:///" + global_args.resource_dir.replace("/\\/g", "/") + '/logo-small.png'
                    smooth: true
                }

                Text {
                    id: versiontext
                    anchors {
                        top: logo.bottom
                        horizontalCenter: logo.horizontalCenter
                        topMargin: 6
                    }
                    text: 'ShoopDaLoop v' + global_args.version_string
                    onLinkActivated: Qt.openUrlExternally(link)
                    color: Material.foreground
                    font.pixelSize: 12
                    linkColor: 'red'
                }
            }

            ToolbarButtonBase {
                id: dsp_indicator

                anchors{
                    right: parent.right
                    bottom: parent.bottom
                }

                implicitWidth: dsprow.width + 8

                Menu {
                    id: dspmenu
                    MenuItem {
                        text: "Reset xruns"
                        onTriggered: session_backend.xruns = 0
                    }
                }

                onClicked: dspmenu.popup()

                Row {
                    id: dsprow
                    spacing: 4
                    anchors.centerIn: parent
                    Label {
                        anchors.verticalCenter: parent.verticalCenter
                        text: "DSP"
                        font.pixelSize: 14
                    }
                    ProgressBar {
                        anchors.verticalCenter: parent.verticalCenter
                        width: 80
                        from: 0.0
                        to: 100.0
                        value: session_backend.dsp_load
                    }
                    Label {
                        anchors.verticalCenter: parent.verticalCenter
                        text: "(" + session_backend.xruns.toString() + ")"
                        font.pixelSize: 14
                    }
                }
            }

            ToolbarRectangle {
                id: latency_indicator
                implicitWidth: buflabel.width + 6

                property int buffer_size: session_backend.buffer_size
                property real latency: session_backend.buffer_size * 1000.0 / session_backend.sample_rate

                anchors {
                    right: dsp_indicator.left
                    bottom: parent.bottom
                }

                Label {
                    id: buflabel
                    anchors.centerIn: parent
                    text: `latency: ${latency_indicator.buffer_size} frames | ${latency_indicator.latency.toFixed(2)} ms`
                    font.pixelSize: 14
                }
            }
        }
    }
}
