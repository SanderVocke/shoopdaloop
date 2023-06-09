import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15
import QtQuick.Dialogs

import "../generate_session.js" as GenerateSession
import "../backend/frontend_interface/types.js" as Types

Item {
    id: root
    objectName: 'session'

    // The descriptor is an object matching the ShoopDaLoop session JSON
    // schema. The Session object will manage an actual session (consisting)
    // of loops, tracks, etc.) to match the loaded descriptor.
    // The descriptor may not be directly modified and is only used at initialization.
    // The actual descriptor can be retrieved with actual_session_descriptor().
    property var initial_descriptor : GenerateSession.generate_session(app_metadata.version_string, [], [], [], [])

    function actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to) {
        return GenerateSession.generate_session(
            app_metadata.version_string,
            tracks_widget.actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to),
            [],
            scenes_widget.actual_scene_descriptors,
            scripting_widget.actual_session_descriptor,
            fx_chain_states_registry.all_values()
        );
    }

    SchemaCheck {
        descriptor: root.initial_descriptor
        schema: 'session.1'
        id: validator
    }

    readonly property bool saving : state_registry.n_saving_actions_active > 0
    readonly property bool loading : state_registry.n_loading_actions_active > 0
    readonly property bool doing_io : saving || loading

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

    // The main registry stores various important internal states.
    // Examples of stuff stored in here:
    // - selected / targeted / master loops
    // - sub-registries, such as for cached FX chain states.
    property Registry state_registry: StateRegistry {
        verbose: false
    }

    // The objects registry is a simple map of widget ids (from the session descriptor)
    // to objects representing those in the app (usually widgets.)
    property Registry objects_registry: ObjectsRegistry {
        verbose: false
    }

    // Store a reference to ourselves in the state registry.
    // Also make some sub-registries:
    // - one to keep track of FX chain states caching
    // - one to keep track of any important objects by ID.
    property Registry fx_chain_states_registry: Registry { verbose: true }
    Component.onCompleted: {
        state_registry.register('session', root)
        state_registry.register('fx_chain_states_registry', fx_chain_states_registry)
    }

    // For (test) access
    property alias tracks: tracks_widget.tracks
    property bool loaded : tracks_widget.loaded

    TasksFactory { id: tasks_factory }

    function save_session(filename) {
        state_registry.save_action_started()
        var tempdir = file_io.create_temporary_folder()
        var tasks = tasks_factory.create_tasks_obj(root)

        var descriptor = actual_session_descriptor(true, tempdir, tasks)
        var session_filename = tempdir + '/session.json'

        // TODO make this step asynchronous
        file_io.write_file(session_filename, JSON.stringify(descriptor, null, 2))

        tasks.when_finished(() => {
            try {
                // TODO make this step asynchronous
                file_io.make_tarfile(filename, tempdir, false)
                console.log("Session written to: ", filename)
            } finally {
                state_registry.save_action_finished()
                file_io.delete_recursive(tempdir)
                tasks.destroy()
            }
        })
    }

    function reload() {
        state_registry.clear([
            'sync_active',
            'scenes_widget'
        ])
        state_registry.reset_saving_loading()
        objects_registry.clear()
        tracks_widget.reload()
    }

    function queue_load_tasks(data_files_directory, add_tasks_to) {
        tracks_widget.queue_load_tasks(data_files_directory, add_tasks_to)
    }

    function load_session(filename) {
        state_registry.load_action_started()
        var tempdir = file_io.create_temporary_folder()

        try {
            var tasks = tasks_factory.create_tasks_obj(root)

            file_io.extract_tarfile(filename, tempdir)

            var session_filename = tempdir + '/session.json'
            var session_file_contents = file_io.read_file(session_filename)
            var descriptor = JSON.parse(session_file_contents)

            schema_validator.validate_schema(descriptor, validator.schema)
            root.initial_descriptor = descriptor
            reload()

            queue_load_tasks(tempdir, tasks)

            tasks.when_finished(() => {
                try {
                    file_io.delete_recursive(tempdir)
                } finally {
                    state_registry.load_action_finished()
                }
            })
            
        } catch(e) {
            file_io.delete_recursive(tempdir)
            throw e;
        }
    }

    Backend {
        update_interval_ms: 30
        client_name_hint: 'ShoopDaLoop'
        backend_type: Types.BackendType.Jack
        id: backend

        anchors {
            fill: parent
            margins: 6
        }

        AppControls {
            id: app_controls

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
                bottom: tracks_widget.bottom
                rightMargin: 6
            }
        }

        ScriptingWidget {
            id: scripting_widget

            initial_descriptor: root.initial_descriptor.scripts
            objects_registry: root.objects_registry
            state_registry: root.state_registry

            height: 160
            anchors {
                bottom: parent.bottom
                left: scenes_widget.right
                right: parent.right
                topMargin: 4
                leftMargin: 4
            }
        }

        TracksWidget {
            id: tracks_widget

            anchors {
                top: app_controls.bottom
                left: scenes_widget.right
                bottom: scripting_widget.top
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
                top: scenes_widget.bottom
                bottom: parent.bottom
                right: scripting_widget.left
            }

            width: 160

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
                    value: backend.dsp_load
                }

                Label { 
                    text: "Xruns: " + backend.xruns.toString()
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
                    onClicked: backend.xruns = 0
                }
            }
        }
    }
}