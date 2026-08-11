import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Layouts 6.6
import ShoopDaLoop.Rust

ShoopRustFXChainGui {
    id: root
    property bool loaded : (initialized &&
                            audio_input_ports_mapper.loaded &&
                            audio_output_ports_mapper.loaded &&
                            midi_input_ports_mapper.loaded)
    onLoadedChanged: root.logger.debug(`${obj_id}: loaded -> ${loaded}`)

    RequireBackend {}

    readonly property var logger : ShoopRustLogger { name: "Frontend.Qml.FXChain" }

    property var descriptor : null
    property int process_lifecycle: 0
    property int process_generation: 0
    property string process_crash_summary: ''
    property int last_notified_crash_generation: -1

    readonly property string obj_id : descriptor.id
    title: descriptor.title

    function actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to) {
        if (!descriptor) { return null; }
        return {
            'schema': 'fx_chain.1',
            'id': obj_id,
            'title': title,
            'type': descriptor.type,
            'ports': all_ports().map(i => i.actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to)),
            'internal_state': get_state_str()
        }
    }

    readonly property string object_schema : 'fx_chain.1'
    SchemaCheck {
        descriptor: root.descriptor
        schema: root.object_schema
    }

    RegisterInRegistry {
        id: reg_entry
        registry: AppRegistries.objects_registry
        key: root.descriptor.id
        object: root
    }

    Component.onCompleted: {
        if (descriptor) {
            switch(descriptor.type) {
                case "carla_rack": chain_type = ShoopRustConstants.FXChainType.CarlaRack; break;
                case "carla_patchbay": chain_type = ShoopRustConstants.FXChainType.CarlaPatchbay; break;
                case "carla_patchbay_16":
                case "carla_patchbay_16x": chain_type = ShoopRustConstants.FXChainType.CarlaPatchbay16x; break;
                case "test2x2x1": chain_type = ShoopRustConstants.FXChainType.Test2x2x1; break;
            }

            if ('internal_state' in descriptor) {
                var restore = function(state_str = descriptor.internal_state) {
                    root.restore_state(state_str)
                }
                if (initialized) { restore() }
                else { root.initialized_changed.connect(function() { restore() }) }
            }

        } else {
            throw new Error("Completed an FX chain object but no descriptor")
        }
        refresh_process_status()
    }

    onReadyChanged: refresh_process_status()

    function refresh_process_status() {
        process_lifecycle = get_process_lifecycle()
        process_generation = get_process_generation()
        process_crash_summary = get_crash_summary()
        if (process_lifecycle === 3 && process_generation !== last_notified_crash_generation) {
            last_notified_crash_generation = process_generation
            process_crash_dialog.open()
        }
    }

    function open_process_logs() {
        process_log_window.refresh()
        process_log_window.show()
        process_log_window.raise()
        process_log_window.requestActivate()
    }

    function unload() {
        reg_entry.close()
        deinit()
    }

    function all_ports() {
        return [...audio_input_ports_mapper.unsorted_instances.map(l => l.item),
                ...audio_output_ports_mapper.unsorted_instances.map(l => l.item),
                ...midi_input_ports_mapper.unsorted_instances.map(l => l.item)]
    }

    Dialog {
        id: process_crash_dialog
        title: 'Carla worker crashed'
        modal: true
        standardButtons: Dialog.Close

        ColumnLayout {
            Label {
                text: `${root.title} process generation ${root.process_generation} crashed.\n${root.process_crash_summary}`
                wrapMode: Text.WordWrap
                Layout.preferredWidth: 420
            }
            Button {
                text: 'Open process logs'
                onClicked: {
                    process_crash_dialog.close()
                    root.open_process_logs()
                }
            }
        }
    }

    ShoopApplicationWindow {
        id: process_log_window
        title: `${root.title} Carla Process Logs`
        width: 850
        height: 600
        visible: false

        function refresh() {
            stdout_text.text = root.get_stdout_log()
            stderr_text.text = root.get_stderr_log()
            root.refresh_process_status()
        }

        ColumnLayout {
            anchors.fill: parent
            anchors.margins: 8

            Label {
                text: root.process_crash_summary.length > 0
                    ? `Generation ${root.process_generation}: ${root.process_crash_summary}`
                    : `Generation ${root.process_generation}`
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
            }

            SplitView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                orientation: Qt.Horizontal

                ColumnLayout {
                    Label { text: 'stdout' }
                    ScrollView {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        TextArea {
                            id: stdout_text
                            readOnly: true
                            selectByMouse: true
                            textFormat: TextEdit.PlainText
                        }
                    }
                }
                ColumnLayout {
                    Label { text: 'stderr' }
                    ScrollView {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        TextArea {
                            id: stderr_text
                            readOnly: true
                            selectByMouse: true
                            textFormat: TextEdit.PlainText
                        }
                    }
                }
            }

            RowLayout {
                Button { text: 'Refresh'; onClicked: process_log_window.refresh() }
                Button {
                    text: 'Copy stdout'
                    onClicked: {
                        stdout_text.selectAll()
                        stdout_text.copy()
                        stdout_text.deselect()
                    }
                }
                Button {
                    text: 'Copy stderr'
                    onClicked: {
                        stderr_text.selectAll()
                        stderr_text.copy()
                        stderr_text.deselect()
                    }
                }
                Button {
                    text: 'Clear'
                    onClicked: {
                        root.clear_process_logs()
                        process_log_window.refresh()
                    }
                }
                Item { Layout.fillWidth: true }
                Button { text: 'Close'; onClicked: process_log_window.close() }
            }
        }
    }

    Mapper {
        id: audio_input_ports_mapper
        model: descriptor.ports.filter(p => p.schema == 'audioport.1' && p.output_connectability.length == 0)

        property bool loaded : {
            if (unsorted_instances.length != model.length) { return false; }
            var result = true;
            for (var i=0; i<unsorted_instances.length; i++) {
                result = result && unsorted_instances[i].loaded;
            }
            return result
        }

        Loader {
            active: root.initialized
            property bool loaded: active && item.loaded
            property var mapped_item
            property int index

            sourceComponent: AudioPort {
                descriptor: mapped_item
                is_internal: true
                backend: root.backend
                maybe_fx_chain: root
                fx_chain_port_idx: index
            }
        }
    }
    Mapper {
        id: audio_output_ports_mapper
        model: descriptor.ports.filter(p => p.schema == 'audioport.1' && p.input_connectability.length == 0)

        property bool loaded : {
            if (unsorted_instances.length != model.length) { return false; }
            var result = true;
            for (var i=0; i<unsorted_instances.length; i++) {
                result = result && unsorted_instances[i].loaded;
            }
            return result
        }

        Loader {
            active: root.initialized
            property bool loaded: active && item.loaded
            property var mapped_item
            property int index

            sourceComponent: AudioPort {
                descriptor: mapped_item
                is_internal: true
                backend: root.backend
                maybe_fx_chain: root
                fx_chain_port_idx: index
            }
        }
    }
    Mapper {
        id: midi_input_ports_mapper
        model: descriptor.ports.filter(p => p.schema == 'midiport.1')

        property bool loaded : {
            if (unsorted_instances.length != model.length) { return false; }
            var result = true;
            for (var i=0; i<unsorted_instances.length; i++) {
                result = result && unsorted_instances[i].loaded;
            }
            return result
        }

        Loader {
            active: root.initialized
            property bool loaded: active && item.loaded
            property var mapped_item
            property int index

            sourceComponent: MidiPort {
                descriptor: mapped_item
                is_internal: true
                backend: root.backend
                maybe_fx_chain: root
                fx_chain_port_idx: index
            }
        }
    }
}