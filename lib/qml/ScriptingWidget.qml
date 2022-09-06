import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

Rectangle {
    id: widget
    color: "#555555"

    property var sections : []
    property int selected_section: -1
    property var scene_names: []

    signal request_change_section_scene(int section_idx, int scene_idx)
    signal request_rename_section(int section_idx, string name)
    signal request_select_section(int section_idx)

//    function onSelectedSceneChanged(name) {
//        if (items[selected_section].scene_name !== name) {
//            selected_section = -1
//            selected_sectionChanged()
//        }
//    }

//    function onSceneRenamed(old_name, new_name) {
//        var idx
//        var items_changed = false
//        for (idx in items) {
//            if (items[idx].scene_name === old_name) {
//                items[idx].scene_name = new_name
//                items_changed = true
//            }
//        }
//        var available_changed = false
//        for (idx in available_scene_names) {
//            if(available_scene_names[idx] === old_name) {
//                available_scene_names[idx] = new_name
//                available_changed = true
//            }
//        }
//        if (items_changed) { itemsChanged() }
//        if (available_changed) { available_scene_namesChanged() }
//    }

//    function onSelected_sectionChanged() {
//        if (selected_section === -1) {
//            console.log('select', '')
//        } else {
//            select_scene(items[selected_section].scene_name)
//            console.log('select', items[selected_section].scene_name)
//        }
//    }

    Item {
        anchors.fill: parent
        anchors.margins: 6

        Item {
            anchors {
                top: parent.top
                bottom: parent.bottom
                left: parent.left
            }
            width: 80

            id: scriptingcontrol

            Text {
                id: scriptingtext

                anchors {
                    top: parent.top
                    left: parent.left
                    right: parent.right
                }

                text: "Scripting"
                font.pixelSize: 15
                color: Material.foreground
                verticalAlignment: Text.AlignTop
                anchors {
                    top: parent.top
                    bottom: parent.bottom
                }
            }
        }

        Rectangle {
            anchors {
                top: parent.top
                bottom: parent.bottom
                left: scriptingcontrol.right
                leftMargin: 6
                right: parent.right
            }
            width: 360
            border.color: 'grey'
            border.width: 1
            color: 'transparent'

            ScrollView {
                anchors {
                    fill: parent
                    margins: parent.border.width + 1
                }

                ScrollBar.horizontal.policy: ScrollBar.AsNeeded
                ScrollBar.vertical.policy: ScrollBar.AlwaysOff

                Row {
                    spacing: 1
                    anchors.fill: parent

                    Repeater {
                        model: widget.sections ? widget.sections.length : 0
                        anchors.fill: parent

                        ScriptItemWidget {
                            is_selected: widget.selected_section === index
                            name: widget.sections[index].name
                            available_scene_names: widget.scene_names
                            selected_scene: widget.sections[index].scene_idx

                            anchors {
                                top: parent.top
                                bottom: parent.bottom
                            }

                            width: 100

                            Connections {
                                function onRequest_rename(name) { widget.request_rename_section(index, name) }
                                function onClicked() {
                                    if (widget.selected_section === index) {
                                        widget.request_select_section(-1)
                                    } else {
                                        widget.request_select_section(index)
                                    }
                                }
                                function onRequest_select_scene(idx) { widget.request_change_section_scene(index, idx) }
                            }
                        }
                    }
                }
            }
        }
    }

    component ScriptItemWidget : Rectangle {
        id: scriptitem

        property bool is_selected
        property string name
        property var available_scene_names
        property int selected_scene: -1

        signal clicked()
        signal request_rename(string name)
        signal request_select_scene(int scene)

        color: is_selected ? 'grey' : Material.background
        border.color: is_selected ? 'red' : 'grey'
        border.width: 2

        MouseArea {
            id: marea
            anchors.fill: parent
            onClicked: scriptitem.clicked()
        }

        TextField {
            id: section_name

            anchors {
                left: parent.left
                right: parent.right
                top: parent.top
                leftMargin: 30
                rightMargin: 5
            }
            text: name
            color: is_selected ? Material.background : Material.foreground
            font.pixelSize: 12

            onEditingFinished: () => { scriptitem.request_rename(displayText); background_focus.forceActiveFocus() }
        }

        ComboBox {
            anchors {
                top: section_name.bottom
                left: parent.left
                leftMargin: 3
                right: parent.right
                rightMargin: 3
            }

            id: combo

            height: 30
            font.pixelSize: 12
            currentIndex: selected_scene + 1

            model : ['(none)'].concat(available_scene_names)
            onModelChanged: () => { selected_sceneChanged() }
            onActivated: (idx) => {
                scriptitem.request_select_scene(idx - 1)
            }
        }
    }
}
