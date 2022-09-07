import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

Rectangle {
    id: widget
    color: "#555555"

    property var sections : []
    property int selected_section: -1
    property var scene_names: []
    property var track_names: []

    signal request_change_section_scene(int section_idx, int scene_idx)
    signal request_rename_section(int section_idx, string name)
    signal request_select_section(int section_idx)
    signal request_add_action(int section_idx, string type, int track_idx)
    signal request_remove_action(int section_idx, string type, int track_idx)

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
                            track_names: widget.track_names
                            actions: widget.sections[index].actions

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
                                function onRequest_add_action(type, track_idx) { widget.request_add_action(index, type, track_idx) }
                                function onRequest_remove_action(type, track_idx) { widget.request_remove_action(index, type, track_idx) }
                            }
                        }
                    }
                }
            }
        }
    }

    // Represent one action visually.
    component ActionIcon : MaterialDesignIcon {
        property var action

        name: action[0] === 'record' ? 'record' :
              action[0] === 'fx_live'? 'effect' : ''

        color: action[0] === 'record' ? 'red' :
               action[0] === 'fx_live' ? 'blue' : ''
    }

    // Represent the set of actions to be taken for a given section in the timeline.
    component ActionsWidget : Item {
        id: act_widget
        property var actions: []
        property int icon_size: 32

        signal clicked()

        Button {
            anchors {
                fill: parent
            }

            onClicked: act_widget.clicked()

            Text {
                id: btlabel
                text: 'Act'
                color: Material.foreground
                font.pixelSize: 12
                anchors {
                    left: parent.left
                    top: parent.top
                    bottom: parent.bottom
                    leftMargin: act_widget.actions.length === 0 ? 30 : 10
                }
                verticalAlignment: Text.AlignVCenter
                width: 40
            }

            Repeater {
                model: actions.length
                anchors {
                    left: btlabel.right
                    right: parent.right
                    top: parent.top
                    bottom: parent.bottom
                }

                ActionIcon {
                    action: actions[index]
                    size: act_widget.icon_size
                    anchors {
                        left: btlabel.right
                        leftMargin: index * 10
                        verticalCenter: parent.verticalCenter
                    }
                }
            }
        }
    }

    // A widget to represent a single section item on the sequencing timeline.
    component ScriptItemWidget : Rectangle {
        id: scriptitem

        property bool is_selected
        property string name
        property var available_scene_names
        property var track_names
        property var actions
        property int selected_scene: -1

        signal clicked()
        signal request_rename(string name)
        signal request_select_scene(int scene)
        signal request_add_action(string type, int track)
        signal request_remove_action(string type, int track)

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

            model : ['(scene)'].concat(available_scene_names)
            onModelChanged: () => { selected_sceneChanged() }
            onActivated: (idx) => {
                scriptitem.request_select_scene(idx - 1)
            }
        }

        ActionsWidget {
            anchors {
                top: combo.bottom
                left: parent.left
                right: parent.right
                bottom: parent.bottom
                leftMargin: 3
                rightMargin: 3
            }

            id: actionswidget

            actions: scriptitem.actions
            icon_size: 25

            onClicked: () => { contextmenu.popup() }

            Menu {
                id: contextmenu
                Menu {
                    title: 'Record'
                    Repeater {
                        model: scriptitem.track_names ? scriptitem.track_names.length : 0

                        MenuItem {
                            text: scriptitem.track_names[index]

                            function is_recorded() {
                                var set = new Set(actionswidget.actions)
                                console.log('act', actionswidget.actions)
                                console.log('ref', ['record', index])
                                return set.has(['record', index])
                            }

                            MaterialDesignIcon {
                                name: 'record'
                                color: 'red'
                                visible: is_recorded()

                                anchors {
                                    verticalCenter: parent.verticalCenter
                                    right: parent.right
                                    rightMargin: 20
                                }
                            }

                            Connections {
                                function onClicked() {
                                    if(is_recorded()) { scriptitem.request_remove_action('record', index) }
                                    else { scriptitem.request_add_action('record', index) }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
