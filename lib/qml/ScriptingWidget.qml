import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

Rectangle {
    id: widget
    color: "#555555"

    property var sections : []
    property var scene_names: []
    property var track_names: []
    property var loop_names: []

    property bool script_playing
    property int script_current_cycle
    property int script_length: {
        var l = 0
        for (var i = 0; i < sections.length; i++) {
            l += sections[i].duration
        }
        return l
    }
    property var current_section: {
        var l = 0
        for (var i = 0; i < sections.length; i++) {
            if (script_current_cycle < (l + sections[i].duration)) {
                return {'section': sections[i], 'section_start': l}
            }
            l += sections[i].duration
        }
        return null
    }
    property int cycle_in_current_section: {
        if (current_section['section']) {
            return script_current_cycle - current_section['section_start']
        }
        return -1
    }

    // Section mgmt
    signal request_rename_section(int section_idx, string name)
    signal request_delete_section(int section_idx)
    signal request_add_section()
    signal request_add_action(int section_idx, var action)
    signal request_remove_action(int section_idx, int action_idx)

    // Transport
    signal play()
    signal stop()
    signal set_cycle(int cycle)

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

            Label {
                color: Material.foreground
                text: widget.script_current_cycle.toString() + '/' + widget.script_length.toString()
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
            Button {
                width: 30
                height: 40
                MaterialDesignIcon {
                    size: 20
                    name: 'play'
                    color: Material.foreground
                    anchors.centerIn: parent
                }
                onClicked: widget.play()
            }
            Button {
                width: 30
                height: 40
                MaterialDesignIcon {
                    size: 20
                    name: 'pause'
                    color: Material.foreground
                    anchors.centerIn: parent
                }
                onClicked: widget.stop()
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
                            name: widget.sections[index].name
                            available_scene_names: widget.scene_names
                            track_names: widget.track_names
                            actions: widget.sections[index].actions
                            duration: widget.sections[index].duration

                            anchors {
                                top: parent.top
                                bottom: parent.bottom
                            }

                            width: 150

                            Connections {
                                function onRequest_rename(name) { widget.request_rename_section(index, name) }
                                function onRequest_delete() { widget.request_delete_section(index) }
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

        property bool highlighted
        property string name
        property var available_scene_names
        property var track_names
        property var actions
        property int duration

        signal clicked()
        signal request_rename(string name)
        signal request_delete()
        signal request_add_action(string type, int track)
        signal request_remove_action(string type, int track)

        color: Material.background
        border.color: highlighted ? 'red' : 'grey'
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
                    value: scriptitem.duration
                    from: 1
                    anchors {
                        left: duration_label.right
                        right: parent.right
                        verticalCenter: parent.verticalCenter
                    }
                    height: 30
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
                        property var original_color: '#444444'
                        color: original_color

                        signal flash()
                        onFlash: SequentialAnimation {
                            PropertyAnimation { target: action_item; property: 'color'; to: 'red' }
                            PropertyAnimation { target: action_item; property: 'color'; to: action_item.original_color }
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
                                var scene_name = 'scene' in action_item.action ? widget.scene_names[action_item.action['scene']] : ''
                                var loop_exists = 'track' in action_item.action &&
                                                      'loop' in action_item.action &&
                                                      widget.loop_names.length > action_item.action['track'] &&
                                                      widget.loop_names[action_item.action['track']].length > action_item.action['loop']
                                var loop_name = loop_exists ? widget.loop_names[action_item.action['track']][action_item.action['loop']] : ''
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

        width: 700
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
                    ScriptActionPopupCombo {
                        id: action_type_combo
                        label: 'Type:'
                        setting: 'action_type'
                        model: {
                            'Loop': 'loop',
                            'Scene': 'scene'
                        }
                    }
                    ScriptActionPopupCombo {
                        id: action_combo
                        label: 'Action:'
                        setting: 'action'
                        property var scene_model: {'Play': 'play'}
                        property var loop_model: {
                            'Play': 'play',
                            'Record': 'record',
                            'Stop': 'stop'
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
                Column {
                    ScriptActionPopupCombo {
                        id: scene_combo
                        label: 'Scene:'
                        setting: 'scene'
                        model: create_enumeration(widget.scene_names)
                        visible: action_type_combo.currentValue == 'scene'
                    }
                    ScriptActionPopupCombo {
                        id: track_combo
                        label: 'Track:'
                        setting: 'track'
                        model: create_enumeration(widget.track_names)
                        visible: action_type_combo.currentValue == 'loop'
                    }
                    ScriptActionPopupCombo {
                        id: loop_combo
                        label: 'Loop:'
                        setting: 'loop'
                        property var track: track_combo.currentValue
                        model: widget.loop_names.length > track ?
                                create_enumeration(
                                    widget.loop_names[track].map((name, idx) => {
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
                        visible: action_combo.currentValue == 'play'
                    }
                }
            }
        }
    }

    component ScriptActionPopupCombo: Row {
        spacing: 5
        id: popup_combo
        property string setting
        property alias label: txt.text
        property var model
        property alias currentValue: combo.currentValue
        property alias currentIndex: combo.currentIndex

        function indexOfValue(val) { return combo.indexOfValue(val) }

        Text {
            id: txt
            color: Material.foreground
            horizontalAlignment: Text.AlignRight
            width: action_popup_component.labels_width
            font.pixelSize: action_popup_component.font_pixel_size
            anchors.verticalCenter: combo.verticalCenter
        }
        ComboBox {
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
