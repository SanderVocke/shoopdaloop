import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Layouts 6.6
import QtQuick.Controls.Material 6.6
import Qt.labs.qmlmodels 1.0
import QtQuick.Dialogs

import ShoopDaLoop.PythonLogger

import '../qml_url_to_filename.js' as UrlToFilename

Dialog {
    id: root
    modal: true
    title: 'Settings'
    standardButtons: Dialog.Save | Dialog.Close
    property bool io_enabled: false

    readonly property PythonLogger logger: PythonLogger { name: "Frontend.Qml.SettingsDialog" }

    onAccepted: all_settings.save()

    width: Overlay.overlay ? Overlay.overlay.width - 50 : 800
    height: Overlay.overlay ? Overlay.overlay.height - 50 : 500
    anchors.centerIn: Overlay.overlay ? Overlay.overlay : parent

    Item {
        anchors.fill: parent

        TabBar {
            id: bar
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top

            TabButton {
                text: 'MIDI Control'
            }
            TabButton {
                text: 'LUA scripts'
            }
        }

        StackLayout {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: bar.bottom
            anchors.bottom: parent.bottom
            currentIndex: bar.currentIndex

            MIDISettingsUi {
                id: midi_settings_being_edited
            }
            ScriptSettingsUi {
                id: lua_settings_being_edited
            }
        }
    }

    Settings {
        id: all_settings
        name: "AllSettings"
        schema_name: "settings"
        current_version: 1

        readonly property PythonLogger logger: PythonLogger { name: "Frontend.Qml.AllSettings" }

        function save() {
            if (!io_enabled) return

            logger.debug(() => ("Saving settings."))
            validate()
            settings_io.save_settings(to_dict(), null)
        }

        function load() {
            if (io_enabled) {
                logger.debug(() => ("Loading settings."))
                let loaded_settings = settings_io.load_settings(null)
                if (loaded_settings != null) { from_dict(loaded_settings) }
            }

            lua_settings_being_edited.sync()
        }

        Component.onCompleted: load()

        contents: ({
            'midi_settings': {
                'schema': 'midi_settings.1',
                'configuration': midi_settings.default_contents()
            },
            'script_settings': {
                'schema': 'script_settings.1',
                'configuration': script_settings.default_contents()
            }
        })
    }

    Settings {
        id: midi_settings
        name: 'MIDISettings'
        schema_name: 'midi_settings'
        current_version: 1

        contents: all_settings.contents.midi_settings.configuration

        function default_contents() { return ({
            'autoconnect_input_regexes': [],
            'autoconnect_output_regexes': [],
            'midi_control_configuration': {
                'schema': 'midi_control_configuration.1',
                'configuration': []
            }
        }) }
    }

    component MIDISettingsUi : Item {
        id: midi_settings_ui

        onAutoconnect_input_regexesChanged: midi_settings.contents.autoconnect_input_regexes = autoconnect_input_regexes
        onAutoconnect_output_regexesChanged: midi_settings.contents.autoconnect_output_regexes = autoconnect_output_regexes

        property var autoconnect_input_regexes: midi_settings.contents ? midi_settings.contents.autoconnect_input_regexes : []
        property var autoconnect_output_regexes: midi_settings.contents ? midi_settings.contents.autoconnect_output_regexes : []

        RegisterInRegistry {
            registry: registries.state_registry
            key: 'autoconnect_input_regexes'
            object: autoconnect_input_regexes
        }
        RegisterInRegistry {
            registry: registries.state_registry
            key: 'autoconnect_output_regexes'
            object: autoconnect_output_regexes
        }

        Column {
            id: header

            Label {
                text: 'For detailed information about MIDI control settings, see the <a href="unknown.html">help</a>.'
                onLinkActivated: (link) => Qt.openUrlExternally(link)
            }
        }

        ToolSeparator {
            id: separator

            anchors {
                top: header.bottom
                left: parent.left
                right: parent.right
            }

            orientation: Qt.Horizontal
        }

        ScrollView {            
            anchors {
                top: separator.bottom
                left: parent.left
                right: parent.right
                bottom: parent.bottom
            }

            contentHeight: scrollcontent.height

            ScrollBar.vertical.policy: ScrollBar.AlwaysOn

            Item {
                id: scrollcontent

                anchors {
                    left: parent.left
                    right: parent.right
                    top: parent.top
                }

                height: childrenRect.height
                
                Column {
                    spacing: 10
                    anchors {
                        top:parent.top
                        left: parent.left
                        right: parent.right
                        rightMargin: 25
                    }

                    GroupBox {
                        title: 'JACK control autoconnect'
                        width: parent.width
                        height: Math.max(input_regexes_column.height, output_regexes_column.height) + 50

                        Column {
                            id: input_regexes_column
                            anchors.left: parent.left
                            anchors.right: parent.horizontalCenter
                            spacing: 3

                            Label {
                                text: 'Input device name regexes'
                            }
                            Repeater {
                                model: midi_settings_ui.autoconnect_input_regexes.length

                                Row {
                                    spacing: 10
                                    height: 40

                                    ShoopTextField {
                                        property string controlledState: midi_settings_ui.autoconnect_input_regexes[index]
                                        onControlledStateChanged: () => { text = controlledState }
                                        onTextChanged: () => { midi_settings_ui.autoconnect_input_regexes[index] = text }
                                        Component.onCompleted: () => { text = controlledState }
                                        width: 190
                                    }
                                    ExtendedButton {
                                        tooltip: "Remove."
                                        width: 30
                                        height: 40
                                        MaterialDesignIcon {
                                            size: 20
                                            name: 'delete'
                                            color: Material.foreground
                                            anchors.centerIn: parent
                                        }
                                        onClicked: {
                                            midi_settings_ui.autoconnect_input_regexes.splice(index, 1)
                                        }
                                    }
                                }
                            }
                            ExtendedButton {
                                tooltip: "Add an input port regex."
                                width: 30
                                height: 40
                                MaterialDesignIcon {
                                    size: 20
                                    name: 'plus'
                                    color: Material.foreground
                                    anchors.centerIn: parent
                                }
                                onClicked: {
                                    midi_settings_ui.autoconnect_input_regexes.push('')
                                    midi_settings_ui.autoconnect_input_regexesChanged()
                                }
                            }
                        }

                        Column {
                            id: output_regexes_column
                            anchors.left: parent.horizontalCenter
                            anchors.right: parent.right
                            spacing: 3
                            
                            Label {
                                text: 'Output device name regexes'
                            }
                            Repeater {
                                model: midi_settings_ui.autoconnect_output_regexes.length

                                Row {
                                    spacing: 10
                                    height: 40

                                    ShoopTextField {
                                        property string controlledState: midi_settings_ui.autoconnect_output_regexes[index]
                                        onControlledStateChanged: () => { text = controlledState }
                                        onTextChanged: () => { midi_settings_ui.autoconnect_output_regexes[index] = text }
                                        Component.onCompleted: () => { text = controlledState }
                                        width: 190
                                    }
                                    ExtendedButton {
                                        tooltip: "Remove."
                                        width: 30
                                        height: 40
                                        MaterialDesignIcon {
                                            size: 20
                                            name: 'delete'
                                            color: Material.foreground
                                            anchors.centerIn: parent
                                        }
                                        onClicked: {
                                            midi_settings.autoconnect_output_regexes.splice(index, 1)
                                        }
                                    }
                                }
                            }
                            ExtendedButton {
                                tooltip: "Add an output port regex."
                                width: 30
                                height: 40
                                MaterialDesignIcon {
                                    size: 20
                                    name: 'plus'
                                    color: Material.foreground
                                    anchors.centerIn: parent
                                }
                                onClicked: {
                                    midi_settings_ui.autoconnect_output_regexes.push('')
                                    midi_settings_ui.autoconnect_output_regexesChanged()
                                }
                            }
                        }
                    }

                    GroupBox {
                        title: 'MIDI control triggers'
                        width: parent.width

                        EditMidiControl {
                            id: edit_midi_control
                            width: parent.width

                            Connections {
                                target: edit_midi_control.configuration
                                function onContentsChanged() {
                                    all_settings.contents.midi_settings.configuration.midi_control_configuration = edit_midi_control.configuration.to_dict()
                                }
                            }

                            Connections {
                                target: all_settings
                                function onContentsChanged() {
                                    edit_midi_control.configuration.from_dict(all_settings.contents.midi_settings.configuration.midi_control_configuration)
                                }
                            }

                            RegisterInRegistry {
                                registry: registries.state_registry
                                key: 'midi_control_configuration'
                                object: edit_midi_control.configuration
                            }
                        }
                    }
                }
            }
        }
    }

    Settings {
        id: script_settings
        contents: all_settings.contents.script_settings.configuration

        schema_name: 'script_settings'
        current_version: 1

        function default_contents() { return ({
                'known_scripts': []
            })
        }

        Component.onCompleted: {
            let builtins_dir = file_io.get_scripts_directory() + '/lib/lua/builtins'
            let builtins = file_io.glob(builtins_dir + '/**/*.lua', true)
            let default_run = ['keyboard.lua']
            for (let builtin of builtins) {
                let builtin_name = file_io.basename(builtin)
                var found = false
                for (let script of contents.known_scripts) {
                    if (script.path_or_filename == builtin || script.path_or_filename == builtin_name) {
                        found = true
                        break
                    }
                }
                if (!found) {
                    contents.known_scripts.push({
                        'path_or_filename': builtin,
                        'run': default_run.includes(builtin_name)
                    })
                }
            }
            contentsChanged()
        }
    }

    component ScriptSettingsUi : Item {
        id: script_ui
        property var known_scripts: script_settings.contents ? script_settings.contents.known_scripts : []

        signal updateKnownScripts(var known_scripts)

        onUpdateKnownScripts: (scripts) => {
            all_settings.contents.script_settings.configuration.known_scripts = scripts
            all_settings.contentsChanged()
        }

        RegistryLookup {
            id: lookup_script_manager
            key: 'lua_script_manager'
            registry: registries.state_registry
        }
        property alias script_manager: lookup_script_manager.object

        function builtins_path() {
            return file_io.realpath(file_io.get_scripts_directory() + '/lib/lua/builtins')
        }

        function full_path(script_name) {
            var fullpath = script_name
            if (!file_io.is_absolute(fullpath)) {
                fullpath = builtins_path() + '/' + fullpath
            }
            if (!file_io.exists(fullpath)) {
                return null
            }
            return file_io.realpath(fullpath)
        }

        function status(script_name) {
            let fullpath = full_path(script_name)
            if (fullpath == null) {
                return 'Not found'
            }
            if (script_manager == null) {
                return 'Not loaded'
            }
            return script_manager.get_status(fullpath)
        }

        function restart(script_name) {
            let fullpath = full_path(script_name)
            if (fullpath == null) {
                return
            }
            if (script_manager == null) {
                return
            }
            script_manager.start_script(fullpath)
        }

        function kill(script_name) {
             let fullpath = full_path(script_name)
            if (fullpath == null) {
                return
            }
            if (script_manager == null) {
                return
            }
            script_manager.kill(fullpath)
        }

        function restart_all() {
            if (script_manager == null) {
                onScript_managerChanged.connect(() => { this.restart_all() })
                return
            }
            script_manager.kill_all()
            for (let script of known_scripts) {
                if (script.run) {
                    restart(full_path(script.path_or_filename))
                }
            }
        }

        function sync() {
            if (script_manager == null) {
                onScript_managerChanged.connect(() => { this.sync() })
                return
            }
            var scripts = []
            for (let script of known_scripts) {
                if (script.run) {
                    scripts.push(full_path(script.path_or_filename))
                }
            }
            script_manager.sync(scripts)
        }

        function docstring(script_name) {
            let fullpath = full_path(script_name)
            if (fullpath == null) {
                return null
            }
            let docstring = script_manager ? script_manager.maybe_docstring(fullpath) : ''
            return docstring
        }
        
        function kind(script_name) {
            let fullpath = full_path(script_name)
            if (fullpath == null) {
                return 'n/a'
            }
            let is_builtin = fullpath.startsWith(builtins_path())
            return is_builtin ? 'built-in' : 'user'
        }

        function add_script(filename) {
            var new_known_scripts = JSON.parse(JSON.stringify(known_scripts))
            new_known_scripts.push({
                'path_or_filename': filename,
                'run': false
            })
            updateKnownScripts(new_known_scripts)
        }

        Label {
            id: lua_label
            anchors.top: parent.top
            text: 'For detailed information about Lua settings, see the <a href="unknown.html">help</a>.'
            onLinkActivated: (link) => Qt.openUrlExternally(link)
        }

        ScrollView {
            width: script_ui.width
            anchors.bottom: parent.bottom
            anchors.top: lua_label.bottom

            Column {
                width: script_ui.width
                spacing: 10

                GroupBox {
                    anchors.left: parent.left
                    anchors.right: parent.right

                    Grid {
                        rows: script_ui.known_scripts.length + 1
                        rowSpacing: 5
                        columnSpacing: 15
                        flow: Grid.TopToBottom
                        verticalItemAlignment: Grid.AlignVCenter
                        
                        // column
                        Label {
                            text: 'Name'
                            font.bold: true
                        }
                        Mapper {
                            model : script_ui.known_scripts
                            Label {
                                property var mapped_item
                                property int index
                                text: file_io.basename(mapped_item.path_or_filename)
                            }
                        }

                        // column
                        Label {
                            text: 'Kind'
                            font.bold: true
                        }
                        Mapper {
                            model : script_ui.known_scripts
                            Label {
                                property var mapped_item
                                property int index
                                property string script_name: mapped_item.path_or_filename
                                text: script_ui.kind(script_name)
                                Connections {
                                    target: script_ui.script_manager
                                    function onChanged() { text = Qt.binding(() => script_ui.kind(script_name)) }
                                }
                            }
                        }

                        // column
                        Label {
                            text: 'Run?'
                            font.bold: true
                        }
                        Mapper {
                            model : script_ui.known_scripts
                            ComboBox {
                                height: 40
                                model: ['Yes', 'No']
                                property var mapped_item
                                property int index
                                currentIndex: mapped_item.run ? 0 : 1
                                onActivated: (idx) => {
                                    let running = mapped_item.run
                                    let run = (idx == 0)
                                    if (!running && run) {
                                        script_ui.restart(mapped_item.path_or_filename)
                                    } else if (running && !run) {
                                        script_ui.kill(mapped_item.path_or_filename)
                                    }
                                    var new_known_scripts = JSON.parse(JSON.stringify(script_ui.known_scripts))
                                    new_known_scripts[index].run = run
                                    script_ui.updateKnownScripts(new_known_scripts)
                                }
                            }
                        }

                        // column
                        Label {
                            text: 'Status'
                            font.bold: true
                        }
                        Mapper {
                            model : script_ui.known_scripts
                            Label {
                                property var mapped_item
                                property int index
                                property string script_name: mapped_item.path_or_filename
                                text: script_ui.status(script_name)
                                Connections {
                                    target: script_ui.script_manager
                                    function onChanged() { text = Qt.binding(() => script_ui.status(script_name)) }
                                }
                            }
                        }

                        // column
                        Item {
                            width: 1
                            height: 1
                        }
                        Mapper {
                            model : script_ui.known_scripts
                            Row {
                                spacing: 5
                                property var mapped_item
                                property int index
                                property var maybe_docstring : script_ui.docstring(mapped_item.path_or_filename)
                                property bool is_builtin: script_ui.kind(mapped_item.path_or_filename) == 'built-in'
                                Connections {
                                    target: script_ui.script_manager
                                    function onChanged() { 
                                        maybe_docstring = Qt.binding(() => script_ui.docstring(mapped_item.path_or_filename))
                                        is_builtin = Qt.binding(() => script_ui.kind(mapped_item.path_or_filename) == 'built-in')
                                    }
                                }
                                ExtendedButton {
                                    tooltip: maybe_docstring ? 'Open documentation' : 'No documentation available' 
                                    width: 30
                                    height: 40
                                    enabled: maybe_docstring
                                    MaterialDesignIcon {
                                        size: 20
                                        name: 'help'
                                        color: enabled ? Material.foreground : 'grey'
                                        anchors.centerIn: parent
                                    }
                                    onClicked: {
                                        var window = script_doc_dialog_factory.createObject(root.parent, {
                                            script_name: file_io.basename(mapped_item.path_or_filename),
                                            docstring: maybe_docstring,
                                            visible: true
                                        })
                                    }
                                }

                                ExtendedButton {
                                    tooltip: 'Forget script'
                                    width: 30
                                    height: 40
                                    visible: !is_builtin
                                    MaterialDesignIcon {
                                        size: 20
                                        name: 'eject'
                                        color: Material.foreground
                                        anchors.centerIn: parent
                                    }
                                    onClicked: {
                                        script_ui.kill(mapped_item.path_or_filename)
                                        var new_known_scripts = JSON.parse(JSON.stringify(script_ui.known_scripts))
                                        new_known_scripts.splice(index, 1)
                                        script_ui.updateKnownScripts(new_known_scripts)
                                    }
                                }
                            }
                        }
                    }
                }

                Button {
                    text: 'Add user script'
                    onClicked: userscriptdialog.open()

                    ShoopFileDialog {
                        id: userscriptdialog
                        fileMode: FileDialog.OpenFile
                        acceptLabel: 'Load LUA script'
                        nameFilters: ["LUA script files (*.lua)"]
                        onAccepted: {
                            close()
                            var filename = UrlToFilename.qml_url_to_filename(file.toString());
                            script_ui.add_script(filename)
                        }
                    }
                }
            }
        }
    }

    Component {
        id: script_doc_dialog_factory
        ApplicationWindow {
            id: window
            property string script_name: '<unknown script>'
            property string docstring: ''
            title: `${script_name} documentation`

            width: 500
            height: 500
            minimumWidth: 100
            minimumHeight: 100
            
            Material.theme: Material.Dark

            ScrollView {
                anchors.fill: parent
                id: view
                Label {
                    text: '```\n' + window.docstring + '\n```'
                    textFormat: Text.MarkdownText
                }
            }
        }
    }
}
