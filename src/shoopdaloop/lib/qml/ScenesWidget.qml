import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3

import "../generate_session.js" as GenerateSession

Item {
    id: root

    property var scenes: []
    property var selected_scene_id: null
    property var hovered_scene_id: null

    property var initial_scene_descriptors: []

    property var actual_scene_descriptors: initial_scene_descriptors

    RegisterInRegistry {
        id: reg_entry
        registry: registries.state_registry
        key: 'scenes_widget'
        object: root
    }

    RegisterInRegistry {
        registry: registries.state_registry
        key: "scene_descriptors"
        object: root.actual_scene_descriptors
    }

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

    function select_scene(actual_descriptor) {
        selected_scene_id = actual_descriptor ? actual_descriptor.id : null
        state_registry.replace ('selected_scene_loop_ids', actual_descriptor ? new Set(actual_descriptor.loop_ids) : new Set())
    }

    function select_scene_loops(actual_descriptor) {
        selected_scene_id = actual_descriptor ? actual_descriptor.id : null
        state_registry.replace ('selected_loop_ids', actual_descriptor ? new Set(actual_descriptor.loop_ids) : new Set())
    }

    function hover_scene(actual_descriptor) {
        hovered_scene_id = actual_descriptor ? actual_descriptor.id : null
        state_registry.replace ('hovered_scene_loop_ids', actual_descriptor ? new Set(actual_descriptor.loop_ids) : new Set())
    }

    function selected_scene() {
        return selected_scene_id ? actual_scene_descriptors.find(d => d.id == selected_scene_id) : null
    }

    function toggle_loop_in_current_scene(loop_id) {
        var scn = selected_scene()
        if (scn) {
            if (scn.loop_ids.includes(loop_id)) {
                scn.loop_ids = scn.loop_ids.filter(i => i != loop_id)
            } else {
                scn.loop_ids.push(loop_id)
                select_scene(scn)
            }
            actual_scene_descriptorsChanged()
            select_scene(scn)
        }
    }

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

                                descriptor: mapped_item

                                anchors {
                                    right: column.right
                                    left: column.left
                                }

                                height: 40
                                name: descriptor.name
                                is_selected: root.selected_scene_id == descriptor.id
                                property bool is_hovered: root.hovered_scene_id == descriptor.id

                                Connections {
                                    function onLeftClicked() {
                                        var to_select = is_selected ? null : descriptor
                                        root.select_scene (to_select)
                                        if (to_select) { root.select_scene_loops (to_select) }
                                    }
                                    function onHoverEntered() { root.hover_scene(descriptor) }
                                    function onHoverExited() { 
                                        if (is_hovered) {
                                            root.hover_scene(null);
                                        }
                                    }
                                    function onNameEntered(name) {
                                        descriptor.name = name;
                                        root.scene_changed(descriptor)
                                    }
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

                ExtendedButton {
                    tooltip: "Add a new scene."
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
                ExtendedButton {
                    tooltip: "Delete current scene."
                    width: 30
                    height: 40
                    MaterialDesignIcon {
                        size: 20
                        name: 'delete'
                        color: Material.foreground
                        anchors.centerIn: parent
                    }
                    onClicked: () => {
                        console.log("TODO")
                    }
                }
            }
        }
    }

    component SceneWidget : Rectangle {
        id: scenewidget

        property bool is_selected
        property string name
        property var descriptor

        //Array of [track, loop]
        property var referenced_loops: []

        signal leftClicked()
        signal middleClicked()
        signal hoverEntered()
        signal hoverExited()
        signal nameEntered(string name)

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

        ShoopTextField {
            width: parent.width - 25
            height: parent.height - 6
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.horizontalCenterOffset: 5
            font.pixelSize: 11

            text: name
            color: is_selected ? Material.background : Material.foreground
            anchors.centerIn: parent

            onEditingFinished: () => { nameEntered(displayText); focus = false }
        }
    }
}
