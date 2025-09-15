import ShoopDaLoop.Rust
import QtQuick 6.6

import 'js/schema_conversions.js' as Conversions

ShoopRustLoopChannelGui {
    id: root
    objectName: "LoopAudioChannel"

    RequireBackend {}

    property var descriptor : null
    readonly property string obj_id : descriptor.id
    instance_identifier: obj_id

    readonly property string object_schema : 'channel.1'
    SchemaCheck {
        descriptor: root.descriptor
        schema: root.object_schema
    }

    readonly property ShoopRustLogger logger : ShoopRustLogger { name: "Frontend.Qml.LoopAudioChannel" }

    function actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to) {
        var rval = {
            'schema': 'channel.1',
            'id': obj_id,
            'mode': initialized ? Conversions.stringify_channel_mode(mode) : descriptor.mode,
            'type': 'audio',
            'data_length': data_length,
            'start_offset': start_offset,
            'n_preplay_samples': n_preplay_samples,
            'gain': audio_gain,
            'connected_port_ids': initialized ? connected_ports.map((c) => c.obj_id) : descriptor.connected_port_ids
        }

        if (recording_started_at) { rval['recording_started_at'] = recording_started_at }
        if (recording_fx_chain_state_id) { rval['recording_fx_chain_state_id'] = recording_fx_chain_state_id }

        if (do_save_data_files && data_length > 0) {
            var filename = obj_id + '.flac'
            var full_filename = data_files_dir + '/' + filename
            var create_task = () => {
                var task = ShoopRustFileIO.save_channels_to_soundfile_async(full_filename, root.backend.sample_rate, [root])
                task.then_delete()
                return task
            }

            if (add_tasks_to) {
                if (add_tasks_to) { add_tasks_to.add_task(create_task()) }
            } else {
                AppRegistries.state_registry.set_active_io_task_fn(create_task)
            }
            rval['data_file'] = filename
        }
        return rval
    }
    function queue_load_tasks(data_files_dir, from_sample_rate, to_sample_rate, add_tasks_to) {
        if (has_data_file()) {
            var create_task = () => {
                var task = ShoopRustFileIO.load_soundfile_to_channels_async(
                    data_files_dir + '/' + descriptor.data_file,
                    to_sample_rate,
                    descriptor.data_length,
                    [[root]],
                    descriptor.n_preplay_samples,
                    descriptor.start_offset,
                    null,
                    3000)
                task.then_delete()
                return task
            }

            if (add_tasks_to) {
                add_tasks_to.add_task(create_task())
            } else {
                AppRegistries.state_registry.set_active_io_task_fn(create_task)
            }
        }
    }
    function has_data_file() {
        return Object.keys(descriptor).includes("data_file")
    }
    function is_empty() {
        return data_length == 0
    }

    function push_gain(gain) {
        push_audio_gain(gain)
    }

    function load_data(data) { load_audio_data(data) }
    function get_data() {
        let rval = get_audio_data()
        return rval
    }

    readonly property int initial_mode : Conversions.parse_channel_mode(descriptor.mode)
    readonly property real initial_gain : ('gain' in descriptor) ? descriptor.gain : 1.0

    onInitial_modeChanged: push_mode(initial_mode)
    onInitial_gainChanged: push_gain(initial_gain)
    ports_to_connect: lookup_connected_ports.objects
    property var recording_fx_chain_state_id: ('recording_fx_chain_state_id' in descriptor) ? descriptor.recording_fx_chain_state_id : null
    recording_started_at: 'recording_started_at' in descriptor ? descriptor.recording_started_at : null
    is_midi: false

    RegistryLookups {
        id: lookup_connected_ports
        registry: AppRegistries.objects_registry
        keys: descriptor.connected_port_ids
    }

    RegisterInRegistry {
        id: reg_entry
        object: root
        key: root.descriptor.id
        registry: AppRegistries.objects_registry
    }

    Component.onCompleted: {
        root.logger.debug(`Created with ${JSON.stringify(descriptor)}`)
        push_mode(initial_mode)
        push_gain(initial_gain)
    }
    function unload() {
        reg_entry.close()
        close()
        destroy()
    }
}
