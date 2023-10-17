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
            registries.fx_chain_states_registry.all_values()
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
        registries.objects_registry.clear()
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
    property list<var> selected_loops : selected_loop_ids ? Array.from(selected_loop_ids).map((id) => registries.objects_registry.get(id)) : []

    RegistryLookup {
        id: targeted_loop_lookup
        registry: state_registry
        key: 'targeted_loop'
    }
    property alias targeted_loop : targeted_loop_lookup.object

    SessionControlInterface {
        id: control_interface
        session: root
    }

    RegisterInRegistry {
        registry: registries.state_registry
        key: 'control_interface'
        object: control_interface
    }

    LuaUserScript {
        when: control_interface.ready
        script_name: 'keyboard.lua'
        script_code: control_interface.ready ? file_io.read_file(
            file_io.get_installation_directory() + '/lib/lua/builtins/keyboard.lua'
        ) : null
    }

    MidiControl {
        id: midi_control
        when: control_interface.ready
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

        MidiControlPort {
            id: midi_control_port
            name_hint: "control"
            direction: Types.PortDirection.Input
            autoconnect_regexes: ['.*APC MINI MIDI.*']

            onMsgReceived: msg => midi_control.handle_midi(msg)
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
        }

        ScenesWidget {
            id: scenes_widget

            initial_scene_descriptors: root.initial_descriptor.scenes
            
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