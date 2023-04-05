import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

import "../generate_session.js" as GenerateSession

Item {
    id: root

    property var scenes: []
    property string selected_scene_id: null
    property string hovered_scene_id: null

    property var initial_scene_descriptors: []
    property Registry objects_registry: null
    property Registry state_registry: null

    property var actual_scene_descriptors: initial_scene_descriptors

    function scene_changed(actual_descriptor) {
        var id = actual_descriptor.id
        for(var i=0; i<actual_scene_descriptors.length; i++) {
            if (actual_scene_descriptors[i].id == id) {
                actual_scene_descriptors[i] = actual_descriptor
            }
        }
        actual_scene_descriptorsChanged()
    }

    function add_scene() {
        function generate_scene_id(n) {
            return "scene_" + n.toString()
        }
        var existing_ids = actual_scene_descriptors.map(d => d.id)

        var n = 1
        var id = generate_scene_id(n)
        while(existing_ids.includes(id)) { n++; id = generate_scene_id(n) }

        var descriptor = GenerateSession.generate_scene(id, "New Scene", [])
        actual_scene_descriptors.push(descriptor)
        actual_scene_descriptorsChanged()
    }

    function remove_scene(actual_descriptor) {
        actual_scene_descriptors = actual_scene_descriptors.filter(d => d.id != actual_descriptor.id)
        actual_scene_descriptorsChanged()
    }

    function select_scene(actual_descriptor) { selected_scene_id = actual_descriptor ? actual_descriptor.id : null }
    function hover_scene(actual_descriptor) { hovered_scene_id = actual_descriptor ? actual_descriptor.id : null }

    // signal request_select_scene(int idx)
    // signal request_hover_scene(int idx)
    // signal request_play_scene(int idx)

    Rectangle {
        width: parent.width
        height: parent.height
        anchors.top: parent.top
        property int x_spacing: 8
        property int y_spacing: 4

        color: "#555555"

        Column {
            anchors {
                fill: parent
                leftMargin: 4
                rightMargin: 4
                topMargin: 2
                bottomMargin: 2
            }

            Item {
                height: 40
                anchors {
                    left: parent.left
                    right: parent.right
                }

                Text {
                    text: "Scenes"
                    font.pixelSize: 15
                    color: Material.foreground
                    anchors {
                        centerIn: parent
                    }
                }
            }

            Rectangle {
                anchors {
                    left: parent.left
                    right: parent.right
                }
                height: 230
                border.color: 'grey'
                border.width: 1
                color: 'transparent'

                ScrollView {
                    anchors {
                        fill: parent
                        margins: parent.border.width + 1
                    }

                    ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
                    ScrollBar.vertical.policy: ScrollBar.AsNeeded

                    Column {
                        id: column
                        spacing: 1
                        anchors.fill: parent

                        Mapper {
                            model: root.actual_scene_descriptors

                            SceneWidget {
                                property var mapped_item
                                property int index

                                onParentChanged: console.log("parent:", parent)

                                anchors {
                                    right: column.right
                                    left: column.left
                                }

                                height: 40
                                name: mapped_item.name
                                is_selected: root.selected_scene_id == mapped_item.id
                                property bool is_hovered: root.hovered_scene_id == mapped_item.id

                                Connections {
                                    function onLeftClicked() { root.select_scene (is_selected ? null : mapped_item) }
                                    function onHoverEntered() { root.hover_scene(mapped_item) }
                                    function onHoverExited() { 
                                        if (is_hovered) {
                                            root.hover_scene(null);
                                        }
                                    }
                                    function onNameEntered(name) {
                                        mapped_item.name = name;
                                        root.scene_changed(mapped_item)
                                    }
                                    //function onPlay() { root.request_play_scene(index) }
                                }
                            }
                        }
                    }
                }
            }

            Row {
                anchors {
                    left: parent.left
                    right: parent.right
                }
                spacing: 4

                Button {
                    width: 30
                    height: 40
                    MaterialDesignIcon {
                        size: 20
                        name: 'plus'
                        color: Material.foreground
                        anchors.centerIn: parent
                    }
                    onClicked: root.add_scene()
                }
                Button {
                    width: 30
                    height: 40
                    MaterialDesignIcon {
                        size: 20
                        name: 'delete'
                        color: Material.foreground
                        anchors.centerIn: parent
                    }
                    onClicked: () => {
                        // if(root.selected_scene >= 0) {
                        //     // Remove the currently selected scene
                        //     request_remove_scene(root.selected_scene)
                        // } else if (root.scenes.length > 0) {
                        //     // Remove the last scene in the list
                        //     request_remove_scene(root.scenes.length - 1)
                        // }
                    }
                }
            }
        }
    }

    component SceneWidget : Rectangle {
        id: scenewidget

        property bool is_selected
        property string name

        //Array of [track, loop]
        property var referenced_loops: []

        signal leftClicked()
        signal middleClicked()
        signal hoverEntered()
        signal hoverExited()
        signal nameEntered(string name)
        signal play()

        color: is_selected ? 'grey' : Material.background
        border.color: marea.containsMouse && is_selected ? 'red' :
                      marea.containsMouse ? 'blue' :
                      is_selected ? 'red' :
                      'grey'
        border.width: 2

        MouseArea {
            id: marea
            hoverEnabled: true
            acceptedButtons: Qt.LeftButton | Qt.MiddleButton
            anchors.fill: parent
            onClicked: (event) => {
                if (event.button === Qt.LeftButton) {
                    scenewidget.leftClicked()
                }
                if (event.button === Qt.MiddleButton) {
                    scenewidget.middleClicked()
                }
            }
            onEntered: hoverEntered()
            onExited: hoverExited()
        }

        Button {
            width: 20
            height: 30
            anchors {
                left: parent.left
                verticalCenter: parent.verticalCenter
                leftMargin: 5
            }
            MaterialDesignIcon {
                size: 15
                name: 'play'
                color: 'green'
                anchors.centerIn: parent
            }
            onClicked: scenewidget.play()
        }

        TextField {
            width: parent.width - 40
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.horizontalCenterOffset: 18

            text: name
            color: is_selected ? Material.background : Material.foreground
            anchors.centerIn: parent

            onEditingFinished: () => { nameEntered(displayText); background_focus.forceActiveFocus() }
        }
    }
}
