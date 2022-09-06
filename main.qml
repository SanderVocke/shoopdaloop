import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

import 'lib/qml'
import 'lib/LoopState.js' as LoopState

ApplicationWindow {
    visible: true
    width: 1050
    height: 600
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
                loops_per_track: 8
                loops_of_selected_scene: shared.loops_of_selected_scene
                loops_of_hovered_scene: shared.loops_of_hovered_scene

                Connections {
                    function onRequest_bind_loop_to_scene(track, loop) { shared.bind_loop_to_current_scene(track, loop) }
                    function onRequest_rename(track, name) { shared.rename_track(track, name) }
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
                    function onRequest_select_scene(idx) { shared.select_scene(idx) }
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
                anchors {
                    top: logo.bottom
                    left: logo.left
                    topMargin: 6
                }
                text: 'ShoopDaLoop v0.1' // TODO
                color: Material.foreground
                font.pixelSize: 12
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

                Connections {
                    function onRequest_change_section_scene(section, scene) { shared.change_section_scene(section, scene) }
                    function onRequest_rename_section(idx, name) { shared.rename_section(idx, name) }
                    function onRequest_select_section(idx) { shared.select_section(idx) }
                }
            }
        }
    }
}
