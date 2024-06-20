import ShoopDaLoop.PythonLoopAudioChannel
import QtQuick 6.6
import ShoopDaLoop.PythonLogger

import '../session_schemas/conversions.js' as Conversions
import ShoopConstants

PythonLoopAudioChannel {
    id: root

    property var descriptor : null

    readonly property string obj_id : descriptor.id

    readonly property string object_schema : 'channel.1'
    SchemaCheck {
        descriptor: root.descriptor
        schema: root.object_schema
    }

    readonly property PythonLogger logger : PythonLogger { name: "Frontend.Qml.LoopAudioChannel" }

    function actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to) {
        var rval = {
            'schema': 'channel.1',
            'id': obj_id,
            'mode': initialized ? Conversions.stringify_channel_mode(mode) : descriptor.mode,
            'type': 'audio',
            'data_length': data_length,
            'start_offset': start_offset,
            'n_preplay_samples': n_preplay_samples,
            'gain': gain,
            'connected_port_ids': initialized ? connected_ports.map((c) => c.obj_id) : descriptor.connected_port_ids
        }
        if (recording_started_at) { rval['recording_started_at'] = recording_started_at }
        if (recording_fx_chain_state_id) { rval['recording_fx_chain_state_id'] = recording_fx_chain_state_id }

        if (do_save_data_files && data_length > 0) {
            var filename = obj_id + '.flac'
            var full_filename = data_files_dir + '/' + filename;
            var task = file_io.save_channels_to_soundfile_async(full_filename, get_backend().get_sample_rate(), [root])
            add_tasks_to.add_task(task)
            rval['data_file'] = filename
        }
        return rval
    }
    function queue_load_tasks(data_files_dir, from_sample_rate, to_sample_rate, add_tasks_to) {
        if (has_data_file()) {
            add_tasks_to.add_task(
                file_io.load_soundfile_to_channels_async(
                    data_files_dir + '/' + descriptor.data_file,
                    to_sample_rate,
                    descriptor.data_length,
                    [[root]],
                    descriptor.n_preplay_samples,
                    descriptor.start_offset,
                    null)
            )
        }
    }
    function has_data_file() {
        return Object.keys(descriptor).includes("data_file")
    }
    function is_empty() {
        return data_length == 0
    }

    readonly property int initial_mode : Conversions.parse_channel_mode(descriptor.mode)
    readonly property real initial_gain : ('gain' in descriptor) ? descriptor.gain : 1.0

    onInitial_modeChanged: set_mode(initial_mode)
    onInitial_gainChanged: set_gain(initial_gain)
    ports: lookup_connected_ports.objects
    property var recording_fx_chain_state_id: ('recording_fx_chain_state_id' in descriptor) ? descriptor.recording_fx_chain_state_id : null
    recording_started_at: 'recording_started_at' in descriptor ? descriptor.recording_started_at : null

    RegistryLookups {
        id: lookup_connected_ports
        registry: registries.objects_registry
        keys: descriptor.connected_port_ids
    }

    RegisterInRegistry {
        id: reg_entry
        object: root
        key: root.descriptor.id
        registry: registries.objects_registry
    }

    onLoopChanged: if (loop) { initialize() }
    Connections {
        target: root.loop ? root.loop : null
        function onInitializedChanged() { root.initialize() }
    }
    Component.onCompleted: {
        root.logger.debug(() => `Created with ${JSON.stringify(descriptor)}`)
        set_mode(initial_mode)
        set_gain(initial_gain)
        initialize()
    }
    function qml_close() {
        reg_entry.close()
        close()
    }
}
