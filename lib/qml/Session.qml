import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

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

    // Objects registry just stores object ids -> objects.
    property Registry objects_registry: Registry { verbose: false }

    // State registry stores the following optional states:
    // - "master_loop" -> LoopWidget which holds the master loop
    // - "targeted_loop" -> LoopWidget which is currently targeted
    property Registry state_registry: Registry { verbose: false }

    // For (test) access
    property alias tracks: tracks_widget.tracks
    property bool loaded : tracks_widget.loaded

    function actual_session_descriptor(do_save_data_files, data_files_dir) {
        return {
            'schema': 'session.1',
            'ports': [],
            'tracks': tracks_widget.actual_session_descriptor(do_save_data_files, data_files_dir)
        };
    }

    function save_session(directory) {
        var descriptor = actual_session_descriptor(true, directory)
        var filename = directory + '/session.json'
        file_io.write_file(filename, JSON.stringify(descriptor, null, 2))
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
    }
}