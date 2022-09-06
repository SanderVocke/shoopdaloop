import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

Rectangle {
    id: widget
    color: "#555555"

    property var items : [
        { name: 'Section 1', scene_name: '' },
        { name: 'Section 2', scene_name: '' }
    ]
    property int selected_section: -1
    property var available_scene_names: []

    function onSelectedSceneChanged(name) {
        if (items[selected_section].scene_name !== name) {
            selected_section = -1
            selected_sectionChanged()
        }
    }

    function onSceneRenamed(old_name, new_name) {
        var idx
        var items_changed = false
        for (idx in items) {
            if (items[idx].scene_name === old_name) {
                items[idx].scene_name = new_name
                items_changed = true
            }
        }
        var available_changed = false
        for (idx in available_scene_names) {
            if(available_scene_names[idx] === old_name) {
                available_scene_names[idx] = new_name
                available_changed = true
            }
        }
        if (items_changed) { itemsChanged() }
        if (available_changed) { available_scene_namesChanged() }
    }

    function onSelected_sectionChanged() {
        if (selected_section === -1) {
            console.log('select', '')
        } else {
            select_scene(items[selected_section].scene_name)
            console.log('select', items[selected_section].scene_name)
        }
    }

    signal select_scene(string name)

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
                        model: widget.items.length
                        anchors.fill: parent

                        ScriptItemWidget {
                            is_selected: widget.selected_section === index
                            name: items[index].name
                            available_scene_names: widget.available_scene_names

                            anchors {
                                top: parent.top
                                bottom: parent.bottom
                            }

                            width: 100

                            Connections {
                                function onNameEntered(name) { items[index].name = name; itemsChanged() }
                                function onClicked() {
                                    if (widget.selected_section === index) {
                                        widget.selected_section = -1
                                        widget.selected_sectionChanged()
                                    } else {
                                        widget.selected_section = index
                                        widget.selected_sectionChanged()
                                        widget.onSelected_sectionChanged()
                                    }
                                }
                                function onSceneNameChosen(name) { items[index].scene_name = name; itemsChanged() }
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
        property int chosen_section_idx: -1

        signal clicked()
        signal nameEntered(string name)
        signal sceneNameChosen(string name)

        color: is_selected ? 'grey' : Material.background
        border.color: is_selected ? 'red' : 'grey'
        border.width: 2

        function update_scene_name() {
            if (chosen_section_idx == -1) {
                 scriptitem.sceneNameChosen('')
            } else {
                 scriptitem.sceneNameChosen(available_scene_names[chosen_section_idx])
            }
        }

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

            onEditingFinished: () => { nameEntered(displayText); background_focus.forceActiveFocus() }
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
            currentIndex: chosen_section_idx + 1

            model : ['(none)'].concat(available_scene_names)
            onModelChanged: () => { chosen_section_idxChanged() }
            onActivated: (idx) => {
                chosen_section_idx = idx - 1
                scriptitem.update_scene_name()
            }
        }
    }
}
