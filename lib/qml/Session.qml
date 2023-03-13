import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15
import QtQuick.Dialogs
import Tasks

import "../generate_session.js" as GenerateSession
import "../../build/types.js" as Types

Item {
    id: session

    // The descriptor is an object matching the ShoopDaLoop session JSON
    // schema. The Session object will manage an actual session (consisting)
    // of loops, tracks, etc.) to match the loaded descriptor.
    // The descriptor may not be directly modified and is only used at initialization.
    // To get an up-to-date descriptor of the current state, call generate_descriptor().
    property var initial_descriptor : ({
        'schema': 'session.1',
        'tracks': [],
        'ports': []
    })

    SchemaCheck {
        descriptor: session.initial_descriptor
        schema: 'session.1'
    }

    RegistryLookup {
        id: saving_lookup
        registry: state_registry
        key: 'n_saving_actions_active'
    }
    RegistryLookup {
        id: loading_lookup
        registry: state_registry
        key: 'n_loading_actions_active'
    }
    readonly property bool saving : saving_lookup.object != null && saving_lookup.object > 0
    readonly property bool loading : loading_lookup.object != null && loading_lookup.object > 0
    readonly property bool doing_io : saving || loading
    Connections {
        target: file_io
        function onStartSavingWav() { state_registry.mutate('n_saving_actions_active', (n) => { return n+1 }) }
        function onDoneSavingWav() { state_registry.mutate('n_saving_actions_active', (n) => { return n-1 }) }
    }

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

    // Objects registry just stores object ids -> objects.
    property Registry objects_registry: Registry { verbose: false }

    // State registry stores the following optional states:
    // - "master_loop" -> LoopWidget which holds the master loop
    // - "targeted_loop" -> LoopWidget which is currently targeted
    property Registry state_registry: StateRegistry { verbose: true }

    // For (test) access
    property alias tracks: tracks_widget.tracks
    property bool loaded : tracks_widget.loaded

    function actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to) {
        return {
            'schema': 'session.1',
            'ports': [],
            'tracks': tracks_widget.actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to)
        };
    }

    function save_session(filename) {
        state_registry.mutate('n_saving_actions_active', (n) => { return n + 1 })
        var tempdir = file_io.create_temporary_folder()
        try {
            var tasks = Qt.createQmlObject(`
                import QtQuick 2.0
                import Tasks

                Tasks {
                }
                `,
                this
            );

            var descriptor = actual_session_descriptor(true, tempdir, tasks)
            var session_filename = tempdir + '/session.json'

            // TODO Async
            file_io.write_file(session_filename, JSON.stringify(descriptor, null, 2))

            var on_done = () => {
                try {
                    // TODO Async
                    file_io.make_tarfile(filename, tempdir, false)
                    console.log("Session written to: ", filename)
                } finally {
                    state_registry.mutate('n_saving_actions_active', (n) => { return n - 1 } )
                    file_io.delete_recursive(tempdir)
                }
            }

            if (!tasks.anything_to_do) { on_done(); }
            else {
                tasks.anythingToDoChanged.connect(on_done)
            }
        } finally {
            // TODO
            console.log("Dispatched session save")
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

        ScenesWidget {
            id: scenes_widget
            
            width: 140
            anchors {
                top: parent.top
                left: parent.left
                bottom: tracks_widget.bottom
                rightMargin: 6
                bottomMargin: 4
            }
        }

        ScriptingWidget {
            id: scripting_widget

            height: 160
            anchors {
                bottom: parent.bottom
                left: scenes_widget.right
                right: parent.right
                topMargin: 4
                leftMargin: 6
            }
        }

        TracksWidget {
            id: tracks_widget

            anchors {
                top: parent.top
                left: scenes_widget.right
                bottom: scripting_widget.top
                right: parent.right
                bottomMargin: 4
                leftMargin: 4
            }

            initial_track_descriptors: session.initial_descriptor.tracks
            objects_registry: session.objects_registry
            state_registry: session.state_registry
        }

        AppControls {
            id: app_controls

            anchors {
                top: scenes_widget.bottom
                left: parent.left
                bottom: parent.bottom
                right: scripting_widget.left
                margins: 4
            }

            loading_session: session.loading
            saving_session: session.saving

            onLoadSession: (filename) => {
                console.log("UNIMPLEMENTED")
            }
            onSaveSession: (filename) => session.save_session(filename)
        }
    }
}