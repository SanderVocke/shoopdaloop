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

    Item {
        anchors {
            fill: parent
            margins: 6
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

                num_tracks: 8
                loops_per_track: 8
                loops_of_selected_scene: scenes.referenced_loops_selected_scene
                loops_of_hovered_scene: scenes.referenced_loops_hovered_scene

                onSet_loop_in_scene: (track, loop) => { scenes.set_loop_in_current_scene(track, loop) }
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
                anchors {
                    top: parent.top
                    bottom: parent.bottom
                    left: logo.right
                    leftMargin: 10
                    right: parent.right
                }
            }
        }
    }
}
