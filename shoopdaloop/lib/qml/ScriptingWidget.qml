import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

import "../generate_session.js" as GenerateSession

Rectangle {
    id: root
    color: "#555555"

    property var initial_descriptor
    property Registry objects_registry: null
    property Registry state_registry: null

    property string active_script_id: initial_descriptor.active_script_id
    property var script_descriptors: initial_descriptor.scripts
    onScript_descriptorsChanged: console.log('script_descriptors')

    readonly property var actual_descriptor: GenerateSession.generate_scripts(script_descriptors, active_script_id)

    readonly property int active_script_index : script_descriptors.findIndex(d => d.id == active_script_id)

    property var active_script_descriptor
    Binding on active_script_descriptor {
        value: script_descriptors.find(d => d.id == active_script_id)
        delayed: true
    }

    property var script_names: script_descriptors.map(s => s.name)

    RegistryLookup {
        id: lookup_scene_descriptors
        registry: root.state_registry
        key: 'scene_descriptors'
    }
    property alias scene_descriptors : lookup_scene_descriptors.object

    RegistryLookup {
        id: lookup_track_descriptors
        registry: root.state_registry
        key: 'track_descriptors'
    }
    property alias track_descriptors : lookup_track_descriptors.object

    RegistryLookup {
        id: lookup_master_loop
        registry: root.state_registry
        key: 'master_loop'
    }
    property alias master_loop : lookup_master_loop.object

    function add_script(name) {
        // Generate a unique ID
        var script_ids = script_descriptors.map(s => s.id)
        var i = 0;
        for(; script_ids.includes(name + "_" + i.toString()); i++) {}
        var id = name + "_" + i.toString()

        var script = GenerateSession.generate_script(
            id, name, 0, []
        );
        script_descriptors.push(script)
        script_descriptorsChanged()
    }

    function delete_script_by_index(index) {
        script_descriptors = script_descriptors.filter((s,idx) => idx != index)
        if(!script_descriptors.map(s => s.id).includes(active_script_id)) {
            active_script_id = script_descriptors.length > 0 ?
                script_descriptors[0] : ""
        }
        script_descriptorsChanged()
    }

    function set_active_script_by_index(index) {
        if (index >= 0) {
            active_script_id = script_descriptors[index].id
        } else {
            active_script_id = ""
        }
    }

    function add_element() {
        if (active_script_id == "") { return; }
        script_descriptors[active_script_index].elements.push(GenerateSession.generate_script_element('', 0, []))
        script_descriptorsChanged()
    }

    // property var sections : []
    // property var scene_names: []
    // property var track_names: []
    // property var loop_names: []

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

    ScriptTransport {
        id: transport

        RegisterInRegistry {
            registry: root.state_registry
            key: 'script_transport'
            object: transport
        }

        Connections {
            target: root.master_loop
            function onCycled() { transport.tick() }
        }
    }

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

                    onClicked: new_script_dialog.open()

                    InputDialog {
                        id: new_script_dialog
                        title: "New script name"
                        default_value: "script"
                        onAcceptedInput: name => root.add_script(name)
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

                    onClicked: root.delete_script_by_index(current_script_combo.currentIndex)
                }
            }

            ComboBox {
                id: current_script_combo
                model: root.script_descriptors.map(s => s.name)
                onCurrentIndexChanged: { root.set_active_script_by_index(currentIndex) }
            }

            Label {
                color: Material.foreground
                text: transport.cycle.toString() + '/' + transport.total_cycles.toString()
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
                    onClicked: root.add_element()
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
                id: scriptelems_scroll

                ScrollBar.horizontal.policy: ScrollBar.AsNeeded
                ScrollBar.vertical.policy: ScrollBar.AlwaysOff

                Row {
                    spacing: 1

                    Repeater {
                        id: script_elements
                        property var model_input: root.active_script_descriptor
                        property int script_index: root.active_script_index

                        model: model_input ? model_input.elements.length : 0

                        ScriptElementWidget {
                            script_index : script_elements.script_index
                            elem_index: index
                            
                            height: scriptelems_scroll.height
                            width: 150
                        }
                    }

                    // Repeater {
                    //     model: root.sections ? root.sections.length : 0
                    //     anchors.top: parent.top
                    //     anchors.bottom: parent.bottom

                    //     ScriptElementWidget {
                    //         name: root.sections[index].name
                    //         available_scene_names: root.scene_names
                    //         track_names: root.track_names
                    //         actions: root.sections[index].actions
                    //         duration: root.sections[index].duration
                    //         start_cycle: index < root.section_starts.length ? root.section_starts[index] : -1

                    //         height: scriptitems_scroll.height
                    //         width: 150

                    //         Connections {
                    //             function onRequest_rename(name) { root.request_rename_section(index, name) }
                    //             function onRequest_delete() { root.request_delete_section(index) }
                    //             function onRequest_add_action(type, track_idx) { root.request_add_action(index, type, track_idx) }
                    //             function onRequest_remove_action(type, track_idx) { root.request_remove_action(index, type, track_idx) }
                    //             function onRequest_set_duration(duration) { root.request_set_section_duration(index, duration) }
                    //         }
                    //         Connections {
                    //             target: shared
                    //             function onAction_executed(section, action) {
                    //                 if (section == index) { action_executed(action) }
                    //             }
                    //         }
                    //     }
                    // }
                }
            }
        }
    }

    // A root to represent a single section item on the sequencing timeline.
    component ScriptElementWidget : Rectangle {
        id: scriptelem

        property int script_index
        property int elem_index

        readonly property var script_descriptor: root.script_descriptors[script_index]
        readonly property var descriptor: script_descriptor.elements[elem_index]

        readonly property var duration: descriptor.duration
        readonly property int start_cycle : {
            var r=0
            for(var i=0; i<=index; i++) {
                if (i>0) { r += script_descriptor.elements[i-1].duration}
            }
            return r;
        }

        property bool active: root.script_playing &&
                              root.script_current_cycle >= start_cycle &&
                              root.script_current_cycle < (start_cycle + duration)
        property string name: descriptor.name

        property var available_scene_names
        property var track_names
        property var actions: descriptor.actions

        function rename(name) {
            root.script_descriptors[script_index].elements[elem_index].name = name
            root.script_descriptorsChanged()
        }

        function set_duration(value) {
            root.script_descriptors[script_index].elements[elem_index].duration = duration
            root.script_descriptorsChanged()
        }

        // signal clicked()
        // signal request_rename(string name)
        // signal request_delete()
        // signal request_add_action(string type, int track)
        // signal request_remove_action(string type, int track)
        // signal request_set_duration(int duration)
        // signal action_executed(int idx)

        color: Material.background
        border.color: active ? 'green' : 'grey'
        border.width: 2

        MouseArea {
            id: marea
            anchors.fill: parent
            onClicked: scriptelem.clicked()
        }

        Column {
            id: header
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.leftMargin: 5
            anchors.rightMargin: 5

            TextField {
                id: elem_name

                anchors {
                    left: parent.left
                    right: parent.right
                }
                text: scriptelem.name
                font.pixelSize: 12

                onEditingFinished: {
                    scriptelem.rename(displayText)
                    background_focus.forceActiveFocus()
                }
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
                    property int watchedValue: scriptelem.duration
                    onWatchedValueChanged: value = watchedValue
                    onValueChanged: scriptelem.set_duration(value)
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
                    model: scriptelem.actions.length
                    anchors.left: parent.left
                    anchors.right: parent.right

                    Rectangle {
                        id: action_item
                        anchors.left: parent.left
                        anchors.right: parent.right
                        property var action: scriptelem.actions[index]
                        property var passive_color: '#444444'
                        property var active_color: ('action' in action && action['action'] == 'record') ? 'red' : 'green'
                        height: 20
                        property bool active: false
                        color: passive_color

                        // Flash when the action is executed
                        Connections {
                            target: scriptelem
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
                            text: action_item.action['delay_cycles']
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
                                    case 'playDryThroughWet':
                                        return 'play'
                                    case 'record':
                                    case 'recordN':
                                    case 'recordDryIntoWet':
                                        return 'record'
                                    case 'stop':
                                    case 'stopAll':
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
                                const act = action_item.action
                                switch(act.schema) {
                                    case 'script_loop_action.1':
                                        if (act.target_ids.length > 1) { return "Loops..." }
                                        else if (act.target_ids.length == 1) {
                                            const maybe_loop = root.objects_registry.maybe_get(act.target_ids[0], undefined)
                                            return maybe_loop ? "Lp " + maybe_loop.name : "Unknown Lp"
                                        }
                                        break
                                    case 'script_scene_action.1':
                                        if (act.target_ids.length > 1) { return "Scenes..." }
                                        else if (act.target_ids.length == 1) {
                                            const maybe_scene = root.scene_descriptors.find(s => s.id == act.target_ids[0])
                                            return maybe_scene ? "Scn " + maybe_scene.name : "Unknown Scn"
                                        }
                                        break
                                    case 'script_track_action.1':
                                        if (act.target_ids.length > 1) { return "Tracks..." }
                                        else if (act.target_ids.length == 1) {
                                            const maybe_track = root.track_descriptors.find(s => s.id == act.target_ids[0])
                                            return maybe_track ? "Trk " + maybe_track.name : "Unknown Trk"
                                        }
                                        break
                                    case 'script_global_action.1':
                                        return "All"
                                        break
                                }

                                return ""

                                // var scene_name = 'scene' in action_item.action ? root.scene_names[action_item.action['scene']] : ''
                                // var loop_exists = 'track' in action_item.action &&
                                //                       'loop' in action_item.action &&
                                //                       root.loop_names.length > action_item.action['track'] &&
                                //                       root.loop_names[action_item.action['track']].length > action_item.action['loop']
                                // var loop_name = loop_exists ? root.loop_names[action_item.action['track']][action_item.action['loop']] : ''
                                // if (loop_exists && loop_name == '') {
                                //     loop_name = '(' + (action_item.action['track']).toString() + ', ' + (1+action_item.action['loop']).toString() + ')'
                                // } else {
                                //     loop_name = '"' + loop_name + '"'
                                // }

                                // switch(action_item.action["action_type"]) {
                                //     case "scene":
                                //         return 'Scn "' + scene_name + '"'
                                //     case "loop":
                                //         return 'Loop ' + loop_name + ''
                                // }
                                // return ''
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
                                            scriptelem.actions[index] = action
                                            scriptelem.actionsChanged()
                                        }
                                    }
                                }
                            }
                            MenuItem {
                                text: "Execute"
                                onClicked: {
                                    shared.execute_action(scriptelem.actions[index])
                                }
                            }
                            MenuItem {
                                text: "Delete"
                                onClicked: {
                                    scriptelem.actions.splice(index, 1)
                                    scriptelem.actionsChanged()
                                }
                            }
                            MenuItem {
                                text: "Move Up"
                                onClicked: {
                                    if (index > 0) {
                                        var tmp = scriptelem.actions[index-1]
                                        scriptelem.actions[index-1] = scriptelem.actions[index]
                                        scriptelem.actions[index] = tmp
                                        scriptelem.actionsChanged()
                                    }
                                }
                            }
                            MenuItem {
                                text: "Move Down"
                                onClicked: {
                                    if (index < scriptelem.actions.length - 1) {
                                        var tmp = scriptelem.actions[index+1]
                                        scriptelem.actions[index+1] = scriptelem.actions[index]
                                        scriptelem.actions[index] = tmp
                                        scriptelem.actionsChanged()
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
                    scriptelem.actions.push(action)
                    scriptelem.actionsChanged()
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
                scriptelem.request_delete()
            }

            Text {
                color: Material.foreground
                text: 'Are you sure you want to delete section "' + scriptelem.name + '" from the script?'
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
            var r
            var delay_cycles = parseInt(on_cycle.text)
            switch(action_type_combo.currentValue) {
                case "loop":
                    r = GenerateSession.generate_script_loop_action(
                        [loop_combo.currentValue.id], action_combo.currentValue, delay_cycles, {} 
                    )
                    break
                case "scene":
                    r = GenerateSession.generate_script_scene_action(
                        [scene_combo.currentValue.id], action_combo.currentValue, delay_cycles, {} 
                    )
                    break
                case "track":
                    r = GenerateSession.generate_script_track_action(
                        [track_combo.currentValue.id], action_combo.currentValue, delay_cycles, {} 
                    )
                    break
                case "global":
                    r = GenerateSession.generate_script_global_action(
                        action_combo.currentValue, delay_cycles, {} 
                    )
                    break
            }
            console.log("Created action:", JSON.stringify(r))
            return r
            // var r = {}
            // var combos = [action_type_combo, action_combo, scene_combo, track_combo, loop_combo, stop_others_combo]
            // for (const c of combos) {
            //     if(c.visible) { r[c.setting] = c.currentValue }
            // }
            // r['on_cycle'] = parseInt(on_cycle.text)
            // return r
        }
        
        function reset_action() {
            var combos = [action_type_combo, action_combo, scene_combo, track_combo, loop_combo, stop_others_combo]
            for (const c of combos) {
                c.currentIndex = 0
            }
            on_cycle.text = '0'
        }

        function adopt_action(action) {
            on_cycle.text = action['delay_cycles'].toString()
            switch(action.schema) {
                case 'script_loop_action.1':
                    action_type_combo.currentIndex = action_type_combo.indexOfValue('loop')
                    var loop_id = action.target_ids[0]
                    var loop_track = root.track_descriptors.find(t => t.loops.map(l => l.id).includes(loop_id))
                    track_combo.currentIndex = track_combo.indexOfValue(loop_track.id)
                    loop_combo.currentIndex = loop_combo.indexOfValue(loop_id)
                    action_combo.currentIndex = action_combo.indexOfValue(action.action)
                    break
                case 'script_track_action.1':
                    action_type_combo.currentIndex = action_type_combo.indexOfValue('track')
                    track_combo.currentIndex = track_combo.indexOfValue(action.target_ids[0])
                    action_combo.currentIndex = action_combo.indexOfValue(action.action)
                    break
                case 'script_scene_action.1':
                    action_type_combo.currentIndex = action_type_combo.indexOfValue('scene')
                    scene_combo.currentIndex = scene_combo.indexOfValue(action.target_ids[0])
                    action_combo.currentIndex = action_combo.indexOfValue(action.action)
                    break
                case 'script_global_action.1':
                    action_type_combo.currentIndex = action_type_combo.indexOfValue('global')
                    action_combo.currentIndex = action_combo.indexOfValue(action.action)
                    break
            }
            // var combos = [action_type_combo, action_combo, scene_combo, track_combo, loop_combo, stop_others_combo]
            // for (var c of combos) {
            //     if (c.setting in action) {
            //         c.currentIndex = c.indexOfValue(action[c.setting])
            //     } else {
            //         c.currentIndex = -1
            //     }
            // }
            // on_cycle.text = action['on_cycle'].toString()
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
                    'Scene': 'scene',
                    'Track': 'track',
                    'Global': 'global'
                }
            }
            ScriptActionPopupCombo {
                id: action_combo
                label: 'Action:'
                setting: 'action'
                property var global_model: {
                    'Stop All': 'stopAll'
                }
                property var scene_model: {
                    'Play': 'play'
                }
                property var loop_model: {
                    'Play': 'play',
                    'Record': 'record',
                    'Record N': 'recordN',
                    'Stop': 'stop',
                    'Replace': 'replace',
                    'PlayFX': 'playDryThroughWet',
                    'RecordFX': 'recordDryIntoWet'
                }
                property var track_model: {
                    'Stop all loops': 'stopAll',
                    'Mute': 'mute',
                    'Unmute': 'unmute',
                    'Mute Passthrough': 'mutePassthrough',
                    'Unmute Passthrough': 'unmutePassthrough'
                }

                model: {
                    switch (action_type_combo.currentValue) {
                    case 'scene':
                        return scene_model
                    case 'loop':
                        return loop_model
                    case 'track':
                        return track_model
                    case 'global':
                        return global_model
                    }
                }
            }

    
            ScriptActionPopupCombo {
                id: scene_combo
                label: 'Scene:'
                setting: 'scene'
                model: create_enumeration(root.scene_descriptors.map(s => s.name))
                visible: action_type_combo.currentValue == 'scene'
            }
            ScriptActionPopupCombo {
                id: track_combo
                label: 'Track:'
                setting: 'track'
                model: {
                    var obj = {}
                    root.track_descriptors.forEach(t => obj[t.name] = t)
                    return obj
                }
                visible: ['loop', 'track'].includes(action_type_combo.currentValue)
            }
            ScriptActionPopupCombo {
                id: loop_combo
                label: 'Loop:'
                setting: 'loop'
                property var track: track_combo.currentValue
                model: {
                    if (!track) { return {} }
                    var obj = {}
                    track.loops.forEach(l => obj[l.name] = l)
                    return obj
                }
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
