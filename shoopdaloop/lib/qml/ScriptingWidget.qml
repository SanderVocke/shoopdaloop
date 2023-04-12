import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

Rectangle {
    id: root
    color: "#555555"

    property var initial_descriptor
    property Registry objects_registry: null
    property Registry state_registry: null

    property var descriptor: initial_descriptor

    // property var sections : []
    // property var scene_names: []
    // property var track_names: []
    // property var loop_names: []

    // property bool script_playing
    // property int script_current_cycle
    // property var section_starts
    // property int script_length
    // property int current_section_idx
    // property int cycle_in_current_section

    // Section mgmt
    // signal request_rename_section(int section_idx, string name)
    // signal request_delete_section(int section_idx)
    // signal request_add_section()
    // signal request_add_action(int section_idx, var action)
    // signal request_remove_action(int section_idx, int action_idx)
    // signal request_set_section_duration(int section_idx, int duration)

    // Transport
    // signal play()
    // signal stop()
    // signal set_cycle(int cycle)

    SchemaCheck {
        descriptor: root.initial_descriptor
        schema: 'scripts.1'
        id: validator
    }

    Item {
        anchors.fill: parent
        anchors.margins: 6

        Column {
            anchors {
                top: parent.top
                bottom: parent.bottom
                left: parent.left
            }
            width: 120

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

            Row {
                spacing: 5
                Button {
                    width: 24
                    height: 34
                    MaterialDesignIcon {
                        size: 20
                        name: 'plus'
                        color: Material.foreground
                        anchors.centerIn: parent
                    }
                }
                Button {
                    width: 24
                    height: 34
                    MaterialDesignIcon {
                        size: 20
                        name: 'delete'
                        color: Material.foreground
                        anchors.centerIn: parent
                    }
                }
            }

            ComboBox {
                model: root.descriptor.scripts.map(s => s.name)
            }

            Label {
                color: Material.foreground
                text: root.script_current_cycle.toString() + '/' + root.script_length.toString()
            }

            Row {
                spacing: 5
                Button {
                    width: 24
                    height: 34
                    MaterialDesignIcon {
                        size: 20
                        name: 'plus'
                        color: Material.foreground
                        anchors.centerIn: parent
                    }
                    onClicked: root.request_add_section()
                }
                Button {
                    width: 24
                    height: 34
                    MaterialDesignIcon {
                        size: 20
                        name: 'play'
                        color: Material.foreground
                        anchors.centerIn: parent
                    }
                    onClicked: root.play()
                }
                Button {
                    width: 24
                    height: 34
                    MaterialDesignIcon {
                        size: 20
                        name: 'pause'
                        color: Material.foreground
                        anchors.centerIn: parent
                    }
                    onClicked: root.stop()
                }
                Button {
                    width: 24
                    height: 34
                    MaterialDesignIcon {
                        size: 20
                        name: 'rotate-left'
                        color: Material.foreground
                        anchors.centerIn: parent
                    }
                    onClicked: root.set_cycle(0)
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
                id: scriptitems_scroll

                ScrollBar.horizontal.policy: ScrollBar.AsNeeded
                ScrollBar.vertical.policy: ScrollBar.AlwaysOff

                Row {
                    spacing: 1

                    Repeater {
                        model: root.sections ? root.sections.length : 0
                        anchors.top: parent.top
                        anchors.bottom: parent.bottom

                        ScriptItemWidget {
                            name: root.sections[index].name
                            available_scene_names: root.scene_names
                            track_names: root.track_names
                            actions: root.sections[index].actions
                            duration: root.sections[index].duration
                            start_cycle: index < root.section_starts.length ? root.section_starts[index] : -1

                            height: scriptitems_scroll.height
                            width: 150

                            Connections {
                                function onRequest_rename(name) { root.request_rename_section(index, name) }
                                function onRequest_delete() { root.request_delete_section(index) }
                                function onRequest_add_action(type, track_idx) { root.request_add_action(index, type, track_idx) }
                                function onRequest_remove_action(type, track_idx) { root.request_remove_action(index, type, track_idx) }
                                function onRequest_set_duration(duration) { root.request_set_section_duration(index, duration) }
                            }
                            Connections {
                                target: shared
                                function onAction_executed(section, action) {
                                    if (section == index) { action_executed(action) }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // A root to represent a single section item on the sequencing timeline.
    component ScriptItemWidget : Rectangle {
        id: scriptitem

        property bool active: root.script_playing &&
                              root.script_current_cycle >= start_cycle &&
                              root.script_current_cycle < (start_cycle + duration)
        property string name
        property var available_scene_names
        property var track_names
        property var actions
        property int duration
        property int start_cycle

        signal clicked()
        signal request_rename(string name)
        signal request_delete()
        signal request_add_action(string type, int track)
        signal request_remove_action(string type, int track)
        signal request_set_duration(int duration)
        signal action_executed(int idx)

        color: Material.background
        border.color: active ? 'green' : 'grey'
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
                font.pixelSize: 12

                onEditingFinished: () => { scriptitem.request_rename(displayText); background_focus.forceActiveFocus() }
            }

            Item {
                anchors.left: parent.left
                anchors.right: parent.right
                height: 24
                Label {
                    id: duration_label
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.left: parent.left
                    text: 'Duration:'
                    font.pixelSize: 12
                    color: Material.foreground
                }
                SpinBox {
                    id: spin
                    property int watchedValue: scriptitem.duration
                    onWatchedValueChanged: value = watchedValue
                    onValueChanged: scriptitem.request_set_duration(value)
                    value: watchedValue
                    from: 1
                    anchors {
                        left: duration_label.right
                        right: parent.right
                        verticalCenter: parent.verticalCenter
                    }
                    height: 30
                    font.pixelSize: 12
                }
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
                        property var passive_color: '#444444'
                        property var active_color: ('action' in action && action['action'] == 'record') ? 'red' : 'green'
                        height: 20
                        property bool active: false
                        color: passive_color

                        // Flash when the action is executed
                        Connections {
                            target: scriptitem
                            function onAction_executed(idx) {
                                if (idx == index) { flash_animation.start() }
                            }
                        }
                        SequentialAnimation {
                            id: flash_animation
                            PropertyAnimation { target: action_item; property: 'color'; to: action_item.active_color }
                            PropertyAnimation { target: action_item; property: 'color'; to: action_item.passive_color }
                        }

                        Label {
                            color: Material.foreground
                            text: action_item.action['on_cycle']
                            anchors.verticalCenter: parent.verticalCenter
                            id: on_cycle_label
                        }

                        MaterialDesignIcon {
                            id: action_icon
                            size: 20
                            anchors.verticalCenter: parent.verticalCenter
                            anchors.left: on_cycle_label.right
                            name: {
                                switch (action_item.action["action"]) {
                                    case 'play':
                                        return 'play'
                                    case 'record':
                                        return 'record'
                                    case 'stop':
                                        return 'stop'
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
                                var scene_name = 'scene' in action_item.action ? root.scene_names[action_item.action['scene']] : ''
                                var loop_exists = 'track' in action_item.action &&
                                                      'loop' in action_item.action &&
                                                      root.loop_names.length > action_item.action['track'] &&
                                                      root.loop_names[action_item.action['track']].length > action_item.action['loop']
                                var loop_name = loop_exists ? root.loop_names[action_item.action['track']][action_item.action['loop']] : ''
                                if (loop_exists && loop_name == '') {
                                    loop_name = '(' + (action_item.action['track']).toString() + ', ' + (1+action_item.action['loop']).toString() + ')'
                                } else {
                                    loop_name = '"' + loop_name + '"'
                                }

                                switch(action_item.action["action_type"]) {
                                    case "scene":
                                        return 'Scn "' + scene_name + '"'
                                    case "loop":
                                        return 'Loop ' + loop_name + ''
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
                                onClicked: {
                                    menu_action_popup.adopt_action(action_item.action)
                                    menu_action_popup.open()
                                }
                                ScriptActionPopup {
                                    id: menu_action_popup
                                    title: 'Edit action'
                                    Connections {
                                        target: menu_action_popup
                                        function onAccepted_action(action) {
                                            scriptitem.actions[index] = action
                                            scriptitem.actionsChanged()
                                        }
                                    }
                                }
                            }
                            MenuItem {
                                text: "Execute"
                                onClicked: {
                                    shared.execute_action(scriptitem.actions[index])
                                }
                            }
                            MenuItem {
                                text: "Delete"
                                onClicked: {
                                    scriptitem.actions.splice(index, 1)
                                    scriptitem.actionsChanged()
                                }
                            }
                            MenuItem {
                                text: "Move Up"
                                onClicked: {
                                    if (index > 0) {
                                        var tmp = scriptitem.actions[index-1]
                                        scriptitem.actions[index-1] = scriptitem.actions[index]
                                        scriptitem.actions[index] = tmp
                                        scriptitem.actionsChanged()
                                    }
                                }
                            }
                            MenuItem {
                                text: "Move Down"
                                onClicked: {
                                    if (index < scriptitem.actions.length - 1) {
                                        var tmp = scriptitem.actions[index+1]
                                        scriptitem.actions[index+1] = scriptitem.actions[index]
                                        scriptitem.actions[index] = tmp
                                        scriptitem.actionsChanged()
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        ScriptActionPopup {
            id: action_popup
            title: 'Add action'
            Connections {
                target: action_popup
                function onAccepted_action(action) {
                    scriptitem.actions.push(action)
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
        standardButtons: {
            var any_problems = false

            var combos = [action_type_combo, action_combo, scene_combo, track_combo, loop_combo, stop_others_combo]
            for (const c of combos) {
                if (c.visible && c.currentValue === undefined) {
                    return Dialog.Cancel
                }
            }
            return Dialog.Ok | Dialog.Cancel
        }
        title: "Script action"
        modal: true

        width: 800
        height: 300

        parent: Overlay.overlay
        anchors.centerIn: parent
        
        onAccepted: accepted_action(create_action())
        signal accepted_action(var action)

        onClosed: reset_action()
        
        function create_action() {
            var r = {}
            var combos = [action_type_combo, action_combo, scene_combo, track_combo, loop_combo, stop_others_combo]
            for (const c of combos) {
                if(c.visible) { r[c.setting] = c.currentValue }
            }
            r['on_cycle'] = parseInt(on_cycle.text)
            return r
        }
        
        function reset_action() {
            var combos = [action_type_combo, action_combo, scene_combo, track_combo, loop_combo, stop_others_combo]
            for (const c of combos) {
                c.currentIndex = 0
            }
            on_cycle.text = '0'
        }

        function adopt_action(action) {
            var combos = [action_type_combo, action_combo, scene_combo, track_combo, loop_combo, stop_others_combo]
            for (var c of combos) {
                if (c.setting in action) {
                    c.currentIndex = c.indexOfValue(action[c.setting])
                } else {
                    c.currentIndex = -1
                }
            }
            on_cycle.text = action['on_cycle'].toString()
        }

        function create_enumeration(lst) {
            var r = {}
            for (var idx=0; idx < lst.length; idx++) {
                r[lst[idx]] = idx
            }
            return r
        }

        Grid {
            rows: 3
            spacing: 5
            flow: Grid.TopToBottom

            Row {
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
            ScriptActionPopupCombo {
                id: action_type_combo
                label: 'Type:'
                setting: 'action_type'
                model: {
                    'Loop': 'loop',
                    'Scene': 'scene',
                    'Track': 'track',
                }
            }
            ScriptActionPopupCombo {
                id: action_combo
                label: 'Action:'
                setting: 'action'
                property var scene_model: {
                    'Play': 'play'
                }
                property var loop_model: {
                    'Play': 'play',
                    'Record': 'record',
                    'Stop': 'stop'
                }
                property var track_model: {
                    'Stop all loops': 'stop_all_loops',
                    'Mute': 'mute',
                    'Unmute': 'unmute',
                    'Mute Passthrough': 'mute_passthrough',
                    'Unmute Passthrough': 'unmute_passthrough'
                }

                model: {
                    switch (action_type_combo.currentValue) {
                    case 'scene':
                        return scene_model
                    case 'loop':
                        return loop_model
                    case 'track':
                        return track_model
                    }
                }
            }

    
            ScriptActionPopupCombo {
                width: 100
                id: scene_combo
                label: 'Scene:'
                setting: 'scene'
                model: create_enumeration(root.scene_names)
                visible: action_type_combo.currentValue == 'scene'
            }
            ScriptActionPopupCombo {
                id: track_combo
                label: 'Track:'
                setting: 'track'
                model: create_enumeration(root.track_names)
                visible: action_type_combo.currentValue == 'loop'
            }
            ScriptActionPopupCombo {
                id: loop_combo
                label: 'Loop:'
                setting: 'loop'
                property var track: track_combo.currentValue
                model: root.loop_names.length > track ?
                        create_enumeration(
                            root.loop_names[track].map((name, idx) => {
                                if (name == '') { return '(' + track.toString() + ', ' + (idx+1).toString() + ')' }
                                return name
                            })
                        ) : []
                visible: action_type_combo.currentValue == 'loop'
            }
            ScriptActionPopupCombo {
                id: stop_others_combo
                label: 'Stop other loops:'
                setting: 'stop_others'
                model: {'No':'no', 'In Track':'track', 'All':'all'}
                visible: action_combo.currentValue == 'play' || action_combo.currentValue == 'record'
            }
        }
    }

    component ScriptActionPopupCombo: Item {
        id: popup_combo
        property string setting
        property alias label: txt.text
        property var model
        property alias currentValue: combo.currentValue
        property alias currentIndex: combo.currentIndex
        implicitHeight: Math.max(txt.implicitHeight, combo.implicitHeight)
        implicitWidth: 350

        function indexOfValue(val) { return combo.indexOfValue(val) }

        Text {
            y:0
            anchors.left: parent.left
            id: txt
            color: Material.foreground
            horizontalAlignment: Text.AlignRight
            width: action_popup_component.labels_width
            font.pixelSize: action_popup_component.font_pixel_size
            anchors.verticalCenter: combo.verticalCenter
        }
        ComboBox {
            y:0
            anchors.left: txt.right
            anchors.right: parent.right
            anchors.leftMargin: 5
            textRole: "label"
            valueRole: "value"
            id: combo
            model: Object.entries(popup_combo.model).map((e) => {
                return { 'label': e[0], 'value': e[1] }
            })
            font.pixelSize: action_popup_component.font_pixel_size
        }
    }
}
