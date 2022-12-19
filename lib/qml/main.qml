import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15
import QtQuick.Dialogs

import '../../build/StatesAndActions.js' as StatesAndActions

ApplicationWindow {
    visible: true
    width: 1050
    height: 550
    maximumWidth: width
    maximumHeight: height
    minimumWidth: width
    minimumHeight: height
    title: "ShoopDaLoop"

    Material.theme: Material.Dark

    // Ensure other controls lose focus when clicked outside
    MouseArea {
        id: background_focus
        anchors.fill: parent
        acceptedButtons: Qt.AllButtons
        onClicked: () => { forceActiveFocus() }
    }

    ApplicationSharedState {
        anchors {
            fill: parent
            margins: 6
        }
        id: shared
        objectName: 'app_shared_state'

        TracksWidget {
            // Note the offset of 1 in all the track indexing. This is because we skip
            // the master track, which is the first track in the shared state. It has
            // a separate track widget.
            id: tracks_widget
            anchors {
                top: parent.top
                right: parent.right
            }

            function map_loop_pos(loop_pos) {
                return [loop_pos[0]-1, loop_pos[1]];
            }

            track_names: shared.track_names.slice(1)
            loop_names: shared.loop_names.slice(1)
            loop_managers: shared.loop_managers.slice(1)
            port_managers: shared.port_managers.slice(1)
            first_loop_index: 2
            master_loop_manager: shared.master_loop_manager
            loops_per_track: shared.loops_per_track
            loops_of_selected_scene: shared.loops_of_selected_scene.map(map_loop_pos)
            loops_of_hovered_scene: shared.loops_of_hovered_scene.map(map_loop_pos)

            Connections {
                function onRequest_toggle_loop_in_scene(track, loop) { shared.toggle_loop_in_current_scene(track+1, loop) }
                function onRequest_rename(track, name) { shared.rename_track(track+1, name) }
                function onRequest_select_loop(track, loop) { shared.select_loop(track+1, loop) }
                function onRequest_rename_loop(track, loop, name) { shared.rename_loop(track+1, loop, name) }
                function onRequest_clear_loop(track, loop) { shared.clear_loop(track+1, loop) }
            }
        }

        // Master loop track with only one loop
        TrackWidget {
            id: masterlooptrack
            anchors {
                top: parent.top
                right: tracks_widget.left
                rightMargin: 6
            }

            name: shared.track_names[0]
            loop_names: shared.loop_names[0]
            num_loops: 1
            first_index: 0
            maybe_master_loop_idx: 0
            master_loop_manager: shared.master_loop_manager
            loop_managers: [shared.master_loop_manager]
            ports_manager: shared.port_managers[0]
            loops_of_selected_scene: []
            loops_of_hovered_scene: []
            onRenamed: (name) => shared.rename_track(0, name)
            onRequest_rename_loop: (idx, name) => shared.rename_loop(0, idx, name)
            onRequest_clear_loop: (idx) => shared.clear_loop(0, idx)
        }

        ScenesWidget {
            anchors {
                top: masterlooptrack.bottom
                topMargin: 6
                left: parent.left
                bottom: parent.bottom
                right: masterlooptrack.right
            }

            id: scenes
            scenes: shared.scenes
            selected_scene: shared.selected_scene
            hovered_scene: shared.hovered_scene

            Connections {
                function onRequest_rename_scene(idx, name) { shared.rename_scene(idx, name) }
                function onRequest_add_scene() { shared.add_scene() }
                function onRequest_remove_scene(idx) { shared.remove_scene(idx) }
                function onRequest_select_scene(idx, activate) {
                    shared.select_scene(idx)
                    if (activate) {
                        shared.activate_scene(idx)
                    }
                }
                function onRequest_hover_scene(idx) { shared.hover_scene(idx) }
            }
        }

        ScriptingWidget {
            id: scripting
            anchors {
                top: tracks_widget.bottom
                topMargin: 6
                bottom: parent.bottom
                left: scenes.right
                leftMargin: 6
                right: logo_menu_area.left
                rightMargin: 6
            }

            sections: shared.sections
            selected_section: shared.selected_section
            scene_names: shared.scene_names
            track_names: shared.track_names

            Connections {
                function onRequest_change_section_scene(section, scene) { shared.change_section_scene(section, scene) }
                function onRequest_rename_section(idx, name) { shared.rename_section(idx, name) }
                function onRequest_select_section(idx) { shared.select_section(idx) }
                function onRequest_add_action(section, type, track) { shared.add_action(section, type, track) }
                function onRequest_remove_action(section, type, track) { shared.remove_action(section, type, track) }
            }
        }

        Item {
            id: logo_menu_area

            anchors {
                top: scripting.top
                bottom: parent.bottom
                right: parent.right
            }

            width: 160

            Item {
                width: childrenRect.width
                height: childrenRect.height

                anchors {
                    horizontalCenter: parent.horizontalCenter
                    verticalCenter: parent.verticalCenter
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
                    text: 'ShoopDaLoop v0.1' // TODO
                    onLinkActivated: Qt.openUrlExternally(link)
                    color: Material.foreground
                    font.pixelSize: 12
                    linkColor: 'red'
                }

            }
        }

        Item {
            id: top_left_area

            anchors {
                left: parent.left
                top: parent.top
                right: masterlooptrack.left
                rightMargin: 6
            }

            Column {
                spacing: 0
                anchors.fill: parent

                Button {
                    anchors {
                        left: parent.left
                        right: parent.right
                    }
                    height: 35
                    onClicked: mainmenu.popup()

                    MaterialDesignIcon {
                        size: parent.width - 10
                        anchors.centerIn: parent
                        name: 'dots-vertical'
                        color: Material.foreground
                    }

                    Menu {
                        id: mainmenu

                        MenuItem {
                            text: "Save copy of session (inc. audio)"
                            onClicked: { savesessiondialog.save_audio = true; savesessiondialog.open() }
                        }
                        MenuItem {
                            text: "Save copy of session (no audio)"
                            onClicked: { savesessiondialog.save_audio = false; savesessiondialog.open() }
                        }
                        MenuItem {
                            text: "Load session"
                            onClicked: loadsessiondialog.open()
                        }
                    }

                    FileDialog {
                        property bool save_audio: false
                        id: savesessiondialog
                        fileMode: FileDialog.SaveFile
                        acceptLabel: 'Save'
                        nameFilters: ["ShoopDaLoop session files (*.shl)(*.shl)"]
                        defaultSuffix: 'shl'
                        onAccepted: {
                            var filename = selectedFile.toString().replace('file://', '');
                            shared.save_session(filename, save_audio)
                        }
                    }

                    FileDialog {
                        id: loadsessiondialog
                        fileMode: FileDialog.OpenFile
                        acceptLabel: 'Load'
                        nameFilters: ["ShoopDaLoop session files (*.shl)(*.shl)"]
                        onAccepted: {
                            var filename = selectedFile.toString().replace('file://', '');
                            shared.load_session(filename)
                        }

                    }
                }

                Button {
                    anchors {
                        left: parent.left
                        right: parent.right
                    }
                    height: 35
                    onClicked: settings.open()

                    MaterialDesignIcon {
                        size: parent.width - 10
                        anchors.centerIn: parent
                        name: 'settings'
                        color: Material.foreground
                    }

                    SettingsDialog {
                        id: settings
                        parent: Overlay.overlay
                        x: (parent.width - width) / 2
                        y: (parent.height - height) / 2
                    }
                }
            }
        }
        
        Popup {
            visible: backend_manager.session_saving
            modal: true

            anchors.centerIn: parent

            Text {
                color: Material.foreground
                text: "Saving session..."
            }
        }

        Popup {
            visible: backend_manager.session_loading
            modal: true

            anchors.centerIn: parent

            Text {
                color: Material.foreground
                text: "Loading session..."
            }
        }
    }
}
