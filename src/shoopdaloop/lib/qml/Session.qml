import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6
import QtQuick.Dialogs
import ShoopDaLoop.PythonLogger
import ShoopDaLoop.PythonControlHandler
import ShoopDaLoop.PythonControlInterface

import "../generate_session.js" as GenerateSession
import ShoopConstants

Rectangle {
    id: root
    color: Material.background
    objectName: 'session'

    readonly property PythonLogger logger : PythonLogger { name: "Frontend.Qml.Session" }

    // The descriptor is an object matching the ShoopDaLoop session JSON
    // schema. The Session object will manage an actual session (consisting)
    // of loops, tracks, etc.) to match the loaded descriptor.
    // The descriptor may not be directly modified and is only used at initialization.
    // The actual descriptor can be retrieved with actual_session_descriptor().
    property var initial_descriptor : GenerateSession.generate_session(app_metadata.version_string, null, [], [], [], [])
    property var backend_type : global_args.backend_type
    
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
        if (loaded) {
            if (global_args.load_session_on_startup && !did_auto_load) {
                did_auto_load = true
                var filename = global_args.load_session_on_startup
                auto_session_loader.filename = filename
                root.logger.debug(() => ("Loading session on startup: " + filename))
                auto_session_loader.trigger()
            }
            if (global_args.test_grab_screens) {
                test_grab_screens_and_quit(global_args.test_grab_screens)
            }
            if (global_args.quit_after >= 0.0) {
                root.logger.info(() => `Auto-quit scheduled for ${global_args.quit_after} seconds.`)
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
            root.logger.info(() => "Auto-quitting as per request.")
            Qt.callLater(Qt.quit)
        }
    }

    ExecuteNextCycle {
        id: test_screen_grab_trigger
        property string output_folder
        onExecute: {
            screen_grabber.grab_all(output_folder)
            root.logger.info(() => ("Screenshots written to: " + output_folder + ". Quitting."))
            Qt.callLater(Qt.quit)
        }
    }
    RegistrySelects {
        registry: registries.objects_registry
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
            app_metadata.version_string,
            session_backend.get_sample_rate(),
            track_groups,
            [],
            [],
            [],
            registries.fx_chain_states_registry.all_values()
        );
    }

    readonly property string object_schema : 'session.1'
    SchemaCheck {
        descriptor: root.initial_descriptor
        schema: root.object_schema
        id: validator
    }

    readonly property bool saving : registries.state_registry.n_saving_actions_active > 0
    readonly property bool loading : registries.state_registry.n_loading_actions_active > 0
    readonly property bool doing_io : saving || loading
    readonly property var backend : session_backend
    property alias control_interface: control_interface

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

    TasksFactory { id: tasks_factory }

    function save_session(filename) {
        root.logger.debug(() => `saving session to: ${filename}`)
        registries.state_registry.reset_saving_loading()
        registries.state_registry.save_action_started()
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
                root.logger.info(() => ("Session written to: " + filename))
            } finally {
                registries.state_registry.save_action_finished()
                file_io.delete_recursive(tempdir)
                tasks.parent = null
                tasks.destroy(30)
            }
        })
    }

    function reload() {
        root.logger.debug(() => ("Reloading session"))
        registries.state_registry.clear([
            'sync_active'
        ])
        registries.objects_registry.clear()
        tracks_widget.reload()
        sync_loop_loader.reload()
    }

    function queue_load_tasks(data_files_directory, from_sample_rate, to_sample_rate, add_tasks_to) {
        tracks_widget.queue_load_tasks(data_files_directory, from_sample_rate, to_sample_rate, add_tasks_to)
        if (sync_loop_loader.track_widget) {
            root.logger.debug(() => (`Queue load tasks for sync track`))
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
            root.logger.debug(() => ("Loading session on startup: " + filename))
            auto_session_loader.trigger()
        }
    }

    function load_session(filename, ignore_resample_warning=false) {
        root.logger.debug(() => `loading session: ${filename}`)
        registries.state_registry.reset_saving_loading()
        registries.state_registry.load_action_started()
        var tempdir = file_io.create_temporary_folder()

        try {
            var tasks = tasks_factory.create_tasks_obj(root)

            file_io.extract_tarfile(filename, tempdir)
            root.logger.debug(() => (`Extracted files: ${JSON.stringify(file_io.glob(tempdir + '/*', true), null, 2)}`))

            var session_filename = tempdir + '/session.json'
            var session_file_contents = file_io.read_file(session_filename)
            var descriptor = JSON.parse(session_file_contents)
            let our_sample_rate = session_backend.get_sample_rate()
            let incoming_sample_rate = descriptor.sample_rate

            if (our_sample_rate != incoming_sample_rate && !ignore_resample_warning) {
                confirm_sample_rate_convert_dialog.session_filename = filename
                confirm_sample_rate_convert_dialog.from = incoming_sample_rate
                confirm_sample_rate_convert_dialog.to = our_sample_rate
                registries.state_registry.load_action_finished()
                confirm_sample_rate_convert_dialog.open()
                return;
            }

            try {
                schema_validator.validate_schema(descriptor, validator.schema)
            } catch(err) {
                console.log("Failed session schema validation for loaded session descriptor:\n",
                            "\nobject:\n", JSON.stringify(descriptor, 0, 2), "\nerror:\n", err.message)
            }
            
            if (our_sample_rate != incoming_sample_rate) {
                descriptor = GenerateSession.convert_session_descriptor_sample_rate(descriptor, incoming_sample_rate, our_sample_rate)
            }

            root.initial_descriptor = descriptor
            reload()
            registries.state_registry.load_action_started()

            let finish_fn = () => {
                root.logger.debug(() => ("Queueing load tasks"))
                queue_load_tasks(tempdir, incoming_sample_rate, our_sample_rate, tasks)

                tasks.when_finished(() => {
                    try {
                        file_io.delete_recursive(tempdir)
                    } finally {
                        root.logger.info(() => ("Session loaded from: " + filename))
                        registries.state_registry.load_action_finished()
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

    SelectedLoops { id: selected_loops_lookup }
    property alias selected_loops: selected_loops_lookup.loops

    RegistryLookup {
        id: targeted_loop_lookup
        registry: registries.state_registry
        key: 'targeted_loop'
    }
    property alias targeted_loop : targeted_loop_lookup.object

    RegisterInRegistry {
        registry: registries.state_registry
        key: 'control_interface'
        object: control_interface
    }

    LuaScriptManager {
        // Only connect to the control interface when it is ready
        control_interface: control_interface

        RegisterInRegistry {
            object: parent
            key: 'lua_script_manager'
            registry: registries.state_registry
        }
    }

    MidiControl {
        id: midi_control
        control_interface: control_interface
        configuration: lookup_midi_configuration.object || fallback

        MidiControlConfiguration { id: fallback }

        RegistryLookup {
            registry: registries.state_registry
            key: 'midi_control_configuration'
            id: lookup_midi_configuration
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

        Keys.onPressed: (event) => control_interface && control_interface.key_pressed(event.key, event.modifiers)
        Keys.onReleased: (event) => control_interface && control_interface.key_released(event.key, event.modifiers)

        property var focusItem : Window.activeFocusItem
        onFocusItemChanged: {
            root.logger.debug(() => ("Focus item changed: " + focusItem))
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
        id: session_backend
        driver_setting_overrides: ({})

        SessionControlInterface {
            id: control_interface
            session: root
        }

        MidiControlPort {
            id: midi_control_port
            name_hint: "control"
            direction: ShoopConstants.PortDirection.Input
            lua_engine: midi_control.lua_engine
            
            RegistryLookup {
                id: lookup_autoconnect
                registry: registries.state_registry
                key: 'autoconnect_input_regexes'
            }

            autoconnect_regexes: lookup_autoconnect.object || []
            may_open: true

            onMsgReceived: msg => midi_control.handle_midi(msg, midi_control_port)
        }

        RegisterInRegistry {
            registry: registries.state_registry
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

            loading_session: root.loading
            saving_session: root.saving

            onLoadSession: (filename) => root.load_session(filename)
            onSaveSession: (filename) => root.save_session(filename)
            onProcessThreadSegfault: session_backend.segfault_on_process_thread()
            onProcessThreadAbort: session_backend.abort_on_process_thread()
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

            anchors {
                top: app_controls.bottom
                left: parent.left
                bottom: pane_area.visible ? pane_area.top : bottom_bar.top
                right: logo_menu_area.left
                bottomMargin: 4
                leftMargin: 4
            }

            initial_track_descriptors: root.main_track_descriptors
        }

        ResizeableItem {
            id: pane_area

            readonly property bool open : registries.state_registry.details_open
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
                    registry: registries.state_registry
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
                id: details_toggle
                text: 'details'
                togglable: true
                height: 26

                Component.onCompleted: registries.state_registry.set_details_open(checked)
                onCheckedChanged: registries.state_registry.set_details_open(checked)
                Connections {
                    target: registries.state_registry
                    function onDetails_openChanged() { details_toggle.checked = registries.state_registry.details_open }
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

                function initialize() {
                    if (track_widget) {
                        track_widget.qml_close()
                    }
                    active = false
                    loaded = false
                    initial_descriptor = root.sync_loop_track_descriptor
                    active = Qt.binding(() => initial_descriptor != null)
                }

                Component.onCompleted: { initialize() }
                function reload() { initialize() }

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
