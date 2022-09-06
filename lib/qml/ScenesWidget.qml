import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

Item {
    id: sceneswidget

    property int selected_scene: -1
    property int hovered_scene: -1
    property var items: [
        // { name: "scene_name", loops: [[0, 0], [1,4]] }
    ]

    function get_referenced_loops_for_selected_scene() {
        if (selected_scene >= 0) {
            return items[selected_scene].loops
        }
        return []
    }
    function get_referenced_loops_for_hovered_scene() {
        if (hovered_scene >= 0) {
            return items[hovered_scene].loops
        }
        return []
    }
    function set_loop_in_current_scene(track, loop) {
        if (selected_scene >= 0) {
            // remove any previous setting for this track
            var modified = items[selected_scene].loops
            modified = modified.filter(l => l[0] !== track)
            // add the new setting
            modified.push([track, loop])
            items[selected_scene].loops = modified
            itemsChanged()
        }
    }
    function add_scene() {
        var name_idx = 1
        var candidate
        while (true) {
            candidate = 'scene ' + name_idx.toString()
            name_idx = name_idx + 1
            var found = false
            var idx
            for (idx in items) { if(items[idx].name === candidate) { found = true; break; } }
            if (!found) { break }
        }

        items.push({ name: candidate, loops: [] })
        itemsChanged()
    }
    function remove_selected_scene() {
        if (selected_scene >= 0) {
            items.splice(selected_scene, 1)
            if (hovered_scene > selected_scene) {
                hovered_scene = hovered_scene - 1
                hovered_sceneChanged()
            }
            else if(hovered_scene == selected_scene) {
                hovered_scene = -1
                hovered_sceneChanged()
            }
            selected_scene = -1
            selected_sceneChanged()
            itemsChanged()
        }
    }
    function select_scene_by_name(name) {
        if (name === '') {
            selected_scene = -1;
            selected_sceneChanged()
        } else {
            var idx
            for(idx in items) {
                if (items[idx].name === name) {
                    selected_scene = idx;
                    selected_sceneChanged()
                }
            }
        }
    }

    signal scene_renamed(string old_name, string new_name)

    // Arrays of [track, loop]
    property var referenced_loops_selected_scene: get_referenced_loops_for_selected_scene()
    property var referenced_loops_hovered_scene: get_referenced_loops_for_hovered_scene()

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
                height: 360
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
                        spacing: 1
                        anchors.fill: parent

                        Repeater {
                            model: sceneswidget.items.length
                            anchors.fill: parent

                            SceneWidget {
                                anchors {
                                    right: parent.right
                                    left: parent.left
                                }

                                height: 40
                                name: sceneswidget.items[index].name
                                is_selected: index === sceneswidget.selected_scene

                                Connections {
                                    function onClicked() {
                                        if (sceneswidget.selected_scene === index) {
                                            sceneswidget.selected_scene = -1
                                            sceneswidget.selected_sceneChanged()
                                        } else {
                                            sceneswidget.selected_scene = index
                                            sceneswidget.selected_sceneChanged()
                                        }
                                    }
                                    function onHoverEntered() { sceneswidget.hovered_scene = index; sceneswidget.hovered_sceneChanged() }
                                    function onHoverExited() { if (sceneswidget.hovered_scene === index) { sceneswidget.hovered_scene = -1; sceneswidget.hovered_sceneChanged()} }
                                    function onNameEntered(name) {
                                        var old_name = sceneswidget.items[index].name;
                                        if (name !== old_name) {
                                            sceneswidget.items[index].name = name;
                                            sceneswidget.scene_renamed(old_name, name);
                                            sceneswidget.itemsChanged()
                                        }
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

                Button {
                    width: 30
                    height: 40
                    MaterialDesignIcon {
                        size: 20
                        name: 'plus'
                        color: Material.foreground
                        anchors.centerIn: parent
                    }
                    onClicked: add_scene()
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
                    onClicked: remove_selected_scene()
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

        signal clicked()
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
            anchors.fill: parent
            onClicked: scenewidget.clicked()
            onEntered: hoverEntered()
            onExited: hoverExited()
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
