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

    signal request_rename_section(int section_idx, string name)
    signal request_delete_section(int section_idx)
    signal request_add_section()
    signal request_add_action(int section_idx, var action)
    signal request_remove_action(int section_idx, int action_idx)

    Item {
        anchors.fill: parent
        anchors.margins: 6

        Column {
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
                    left: parent.left
                    right: parent.right
                }

                text: "Scripting"
                font.pixelSize: 15
                color: Material.foreground
                verticalAlignment: Text.AlignTop
            }

            Button {
                    width: 30
                    height: 40
                    MaterialDesignIcon {
                        size: 20
                        name: 'plus'
                        color: Material.foreground
                        anchors.centerIn: parent
                    }
                    onClicked: widget.request_add_section()
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
                            track_names: widget.track_names
                            actions: widget.sections[index].actions

                            anchors {
                                top: parent.top
                                bottom: parent.bottom
                            }

                            width: 150

                            Connections {
                                function onRequest_rename(name) { widget.request_rename_section(index, name) }
                                function onRequest_delete() { widget.request_delete_section(index) }
                                function onClicked() {
                                    if (widget.selected_section === index) {
                                        widget.request_select_section(-1)
                                    } else {
                                        widget.request_select_section(index)
                                    }
                                }
                                function onRequest_add_action(type, track_idx) { widget.request_add_action(index, type, track_idx) }
                                function onRequest_remove_action(type, track_idx) { widget.request_remove_action(index, type, track_idx) }
                            }
                        }
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

        signal clicked()
        signal request_rename(string name)
        signal request_delete()
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

        Column {
            id: header
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.leftMargin: 5
            anchors.rightMargin: 5

            TextField {
                id: section_name

                anchors {
                    left: parent.left
                    right: parent.right
                }
                text: name
                color: is_selected ? Material.background : Material.foreground
                font.pixelSize: 12

                onEditingFinished: () => { scriptitem.request_rename(displayText); background_focus.forceActiveFocus() }
            }

            Row {
                anchors {
                    left: parent.left
                    right: parent.right
                }
                spacing: 3

                Button {
                    width: 60
                    height: 30
                    Row {
                        anchors.centerIn: parent
                        MaterialDesignIcon {
                            size: 15
                            name: 'plus'
                            color: Material.foreground
                        }
                        Text {
                            font.pixelSize: 12
                            color: Material.foreground
                            text: 'Action'
                        }
                    }
                    onClicked: action_popup.open()
                }
                Button {
                    width: 20
                    height: 30
                    MaterialDesignIcon {
                        size: 15
                        name: 'delete'
                        color: Material.foreground
                        anchors.centerIn: parent
                    }
                    onClicked: delete_popup.open()
                }
                Button {
                    width: 20
                    height: 30
                    MaterialDesignIcon {
                        size: 15
                        name: 'play'
                        color: Material.foreground
                        anchors.centerIn: parent
                    }
                }
            }
        }

        ScrollView {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.leftMargin: 5
            anchors.rightMargin: 5
            anchors.top: header.bottom
            anchors.bottom: parent.bottom
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff

            Column {
                anchors.left: parent.left
                anchors.right: parent.right
                spacing: 2

                Repeater {
                    model: scriptitem.actions.length
                    anchors.left: parent.left
                    anchors.right: parent.right

                    Rectangle {
                        id: action_item
                        anchors.left: parent.left
                        anchors.right: parent.right
                        property var action: scriptitem.actions[index]
                        height: 20
                        color: '#444444'

                        MaterialDesignIcon {
                            id: action_icon
                            size: 20
                            anchors.verticalCenter: parent.verticalCenter
                            name: {
                                switch (action_item.action["action"]) {
                                    case 'play':
                                        return 'play'
                                }
                                return ''
                            }
                            color: Material.foreground
                        }

                        Text {
                            color: Material.foreground
                            anchors.left: action_icon.right
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            font.pixelSize: 12

                            text: {
                                switch(action_item.action["action_type"]) {
                                    case "scene":
                                        return 'Scn "' + widget.scene_names[action_item.action['scene']] + '"'
                                }
                                return ''
                            }
                        }

                        MouseArea {
                            anchors.fill: parent
                            acceptedButtons: Qt.LeftButton | Qt.RightButton
                            hoverEnabled: true
                            onClicked: action_menu.popup()
                        }

                        Menu {
                            id: action_menu
                            MenuItem {
                                text: "Edit..."
                            }
                            MenuItem {
                                text: "Delete"
                            }
                            MenuItem {
                                text: "Move Up"
                            }
                            MenuItem {
                                text: "Move Down"
                            }
                        }
                    }
                }
            }
        }

        ScriptActionPopup {
            id: action_popup
            Connections {
                target: action_popup
                function onAdd_action(action) {
                    scriptitem.actions.push(action)
                    console.log(JSON.stringify(scriptitem.actions))
                    scriptitem.actionsChanged()
                }
            }
        }

        Dialog {
            id: delete_popup
            title: 'Warning'
            standardButtons: Dialog.Ok | Dialog.Cancel
            parent: Overlay.overlay
            anchors.centerIn: parent

            onAccepted: {
                close()
                scriptitem.request_delete()
            }

            Text {
                color: Material.foreground
                text: 'Are you sure you want to delete section "' + scriptitem.name + '" from the script?'
            }
        }
    }

    component ScriptActionPopup : Dialog {
        id: action_popup_component
        property int labels_width: 120
        property int font_pixel_size: 14
        standardButtons: Dialog.Ok | Dialog.Cancel
        title: "Add action"
        modal: true

        parent: Overlay.overlay
        anchors.centerIn: parent
        
        onAccepted: add_action(create_action_object())

        signal add_action(var action)

        function create_combo(id, label, item_ids_to_labels) {
            var combo_model = 'ListModel {\n'
            for (const [label, val] of Object.entries(item_ids_to_labels)) {
                var full_val = (typeof val === 'string' || val instanceof String) ? '"' + val + '"' : val
                combo_model += '   ListElement { label: "' + label + '"; value: ' + full_val + ' }\n'
            }
            combo_model += '}'
            var snippet = '
                import QtQuick 2.15
                import QtQuick.Controls 2.15
                import QtQuick.Controls.Material 2.15

                Component {
                    Row {
                        spacing: 5
                        property string setting: "' + id + '"
                        property alias value: ' + id + '.currentValue
                        Text {
                            color: Material.foreground
                            text: "' + label + '"
                            horizontalAlignment: Text.AlignRight
                            width: action_popup_component.labels_width
                            font.pixelSize: action_popup_component.font_pixel_size
                            anchors.verticalCenter: ' + id + '.verticalCenter
                        }
                        ComboBox {
                            textRole: "label"
                            valueRole: "value"
                            id: ' + id + '
                            model: ' + combo_model + '
                            font.pixelSize: action_popup_component.font_pixel_size
                        }
                    }
                }
            '
            return Qt.createQmlObject(snippet, action_popup_component, id)
        }

        function create_action_object() {
            var r = {
                'cycle': parseInt(on_cycle.text),
                'action_type': action_type_combo.currentValue,
                'action': action_combo.currentValue
            }
            for (var idx = 0; idx < combo_instantiations.count; idx++) {
                var combo = combo_instantiations.itemAt(idx).item
                if(combo !== undefined) {
                    r[combo.setting] = combo.value
                }
            }
            return r
        }

        function create_enumeration(lst) {
            var r = {}
            for (var idx=0; idx < lst.length; idx++) {
                r[lst[idx]] = idx
            }
            return r
        }

        property var combo_boxes: {
            var r = []
            switch(action_type_combo.currentValue) {
                case 'scene':
                    r.push(create_combo('scene', 'Scene:', create_enumeration(widget.scene_names)))
                    switch(action_combo.currentValue) {
                        case 'play':
                            r.push(create_combo('stop_others', 'Stop other loops:', {'No':'no', 'Yes':'yes'}))
                            break
                    }                
                    break
                case 'loop':
                    r.push(create_combo('track', 'Track:', {'Track 1': 0, 'Track 2': 1}))
                    r.push(create_combo('loop', 'Loop:', {'Loop 1': 1, 'Loop 2': 2}))
                    switch(action_combo.currentValue) {
                        case 'play':
                            r.push(create_combo('stop_others', 'Stop other loops:', {'No':'no', 'In Track':'track', 'All':'all'}))
                            break
                        case 'record':
                            break
                        case 'stop':
                            break
                    }
                    break
            }

            return r
        }

        Column {
            Row {
                Column {
                    Row {
                        spacing: 5
                        Text {
                            horizontalAlignment: Text.AlignRight
                            width: action_popup_component.labels_width
                            text: 'On Cycle:'
                            color: Material.foreground
                            font.pixelSize: action_popup_component.font_pixel_size
                            anchors.verticalCenter: on_cycle.verticalCenter
                        }
                        TextField {
                            id: on_cycle
                            font.pixelSize: action_popup_component.font_pixel_size
                            text: '0'
                            validator: IntValidator { bottom: 0; top: 100 }
                        }
                    }
                    Row {
                        spacing: 5
                        Text {
                            horizontalAlignment: Text.AlignRight
                            width: action_popup_component.labels_width
                            text: 'Type:'
                            color: Material.foreground
                            font.pixelSize: action_popup_component.font_pixel_size
                            anchors.verticalCenter: action_type_combo.verticalCenter
                        }
                        ComboBox {
                            id: action_type_combo
                            textRole: 'label'
                            valueRole: 'value'
                            model: ListModel {
                                ListElement { label: 'Scene'; value: 'scene' }
                                ListElement { label: 'Loop'; value: 'loop' }
                            }
                            font.pixelSize: action_popup_component.font_pixel_size
                        }
                    }
                    Row {
                        spacing: 5
                        Text {
                            horizontalAlignment: Text.AlignRight
                            width: action_popup_component.labels_width
                            text: 'Action:'
                            color: Material.foreground
                            font.pixelSize: action_popup_component.font_pixel_size
                            anchors.verticalCenter: action_combo.verticalCenter
                        }
                        ComboBox {
                            id: action_combo
                            font.pixelSize: action_popup_component.font_pixel_size
                            textRole: 'label'
                            valueRole: 'value'

                            property var scene_model: ListModel {
                                ListElement { label: 'Play'; value: 'play' }
                            }

                            property var loop_model: ListModel {
                                ListElement { label: 'Play'; value: 'play' }
                                ListElement { label: 'Record'; value: 'record' }
                                ListElement { label: 'Stop'; value: 'stop' }
                            }

                            model: {
                                switch (action_type_combo.currentValue) {
                                case 'scene':
                                    return scene_model
                                case 'loop':
                                    return loop_model
                                }
                            }
                        }
                    }
                }
                Column {
                    Repeater {
                        model: combo_boxes.length
                        id: combo_instantiations

                        Loader {
                            sourceComponent: combo_boxes[index]
                        }
                    }
                }
            }
        }
    }
}
