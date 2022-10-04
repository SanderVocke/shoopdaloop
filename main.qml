import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15
import QtQuick.Dialogs

import 'lib/qml'
import 'lib/LoopState.js' as LoopState

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

        // Tell the global manager what the desired loop count is, so SooperLooper will be set up for it.
        Component.onCompleted: () => {
                                   var num_loops = track_names.length * loops_per_track
                                   sl_global_manager.set_desired_looper_count(num_loops)
                               }

        Item {
            id: tracks_scenes_row

            anchors {
                left: parent.left
                right: parent.right
            }
            height: tracks_widget.height

            TracksWidget {
                id: tracks_widget
                anchors.top: parent.top

                track_names: shared.track_names
                loop_names: shared.loop_names
                loop_managers: shared.loop_managers
                master_loop_manager: shared.master_loop_manager
                master_loop_idx: shared.master_loop_idx
                loops_per_track: shared.loops_per_track
                loops_of_selected_scene: shared.loops_of_selected_scene
                loops_of_hovered_scene: shared.loops_of_hovered_scene
                selected_loops: shared.selected_loops

                Connections {
                    function onRequest_bind_loop_to_scene(track, loop) { shared.bind_loop_to_current_scene(track, loop) }
                    function onRequest_rename(track, name) { shared.rename_track(track, name) }
                    function onRequest_select_loop(track, loop) { shared.select_loop(track, loop) }
                    function onRequest_load_wav(track, loop, wav_file) { shared.load_loop_wav(track, loop, wav_file) }
                    function onRequest_save_wav(track, loop, wav_file) { shared.save_loop_wav(track, loop, wav_file) }
                    function onRequest_rename_loop(track, loop, name) { shared.rename_loop(track, loop, name) }
                }
            }

            ScenesWidget {
                anchors {
                    top: tracks_widget.top
                    bottom: tracks_widget.bottom
                    left: tracks_widget.right
                    leftMargin: 6
                    right: parent.right
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
        }

        Item {
            anchors {
                left: tracks_scenes_row.left
                right: tracks_scenes_row.right
                top: tracks_scenes_row.bottom
                topMargin: 6
                bottom: parent.bottom
            }

            Image {
                id: logo
                anchors {
                    top: parent.top
                    topMargin: 6
                }

                height: 60
                width: height / sourceSize.height * sourceSize.width
                source: 'resources/logo-small.png'
                smooth: true
            }

            Text {
                id: versiontext
                anchors {
                    top: logo.bottom
                    left: logo.left
                    topMargin: 6
                }
                text: 'ShoopDaLoop v0.1' // TODO
                color: Material.foreground
                font.pixelSize: 12
            }

            Button {
                anchors {
                    top: versiontext.bottom
                    left: logo.left
                    right: logo.right
                }
                text: 'Menu'
                height: 35
                onClicked: mainmenu.popup()

                Menu {
                    id: mainmenu
                    MenuItem {
                        text: "Save session (with audio)..."
                        onClicked: { savesessiondialog.save_audio = true; savesessiondialog.open() }
                    }
                    MenuItem {
                        text: "Save session (no audio)..."
                        onClicked: { savesessiondialog.save_audio = false; savesessiondialog.open() }
                    }
                    MenuItem {
                        text: "Load session..."
                        onClicked: loadsessiondialog.open()
                    }
                }

                FileDialog {
                    property bool save_audio: false
                    id: savesessiondialog
                    fileMode: FileDialog.SaveFile
                    acceptLabel: 'Save'
                    nameFilters: ["ShoopDaLoop session files (*.shl)"]
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
                    nameFilters: ["ShoopDaLoop session files (*.shl)"]
                    onAccepted: {
                        var filename = selectedFile.toString().replace('file://', '');
                        shared.load_session(filename)
                    }

                }
            }

            ScriptingWidget {
                id: scripting
                anchors {
                    top: parent.top
                    bottom: parent.bottom
                    left: logo.right
                    leftMargin: 10
                    right: parent.right
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
        }
    }
}
