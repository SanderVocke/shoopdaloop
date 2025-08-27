import ShoopDaLoop.Rust
import QtQuick 6.6

import ShoopConstants
import 'js/schema_conversions.js' as Conversions

LoopChannelGui {
    id: root
    objectName: "LoopMidiChannel"

    RequireBackend {}

    property var descriptor : null

    readonly property string obj_id : descriptor.id
    instance_identifier: obj_id

    readonly property string object_schema : 'channel.1'
    SchemaCheck {
        descriptor: root.descriptor
        schema: root.object_schema
    }

    function is_empty() { return data_length == 0 }

    function actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to) {
        var rval = {
            'schema': 'channel.1',
            'id': obj_id,
            'mode': initialized ? Conversions.stringify_channel_mode(mode) : descriptor.mode,
            'type': 'midi',
            'data_length': data_length,
            'start_offset': start_offset,
            'n_preplay_samples': n_preplay_samples,
            'connected_port_ids': initialized ? connected_ports.map((c) => c.obj_id) : descriptor.connected_port_ids
        }
        if (recording_started_at) { rval['recording_started_at'] = recording_started_at }
        if (recording_fx_chain_state_id) { rval['recording_fx_chain_state_id'] = recording_fx_chain_state_id }

        if (do_save_data_files && !root.empty) {
            var filename = obj_id + '.smf'
            var full_filename = data_files_dir + '/' + filename;

            var create_task = () => {
                var task = ShoopFileIO.save_channel_to_midi_async(full_filename, root.backend.get_sample_rate(), root)
                task.then_delete()
                return task
            }

            if (add_tasks_to) {
                add_tasks_to.add_task(create_task())
            } else {
                registries.state_registry.set_active_io_task_fn(create_task)
            }
            rval['data_file'] = filename
        }
        return rval
    }
    function queue_load_tasks(data_files_dir, from_sample_rate, to_sample_rate, add_tasks_to) {
        const conversion_factor = to_sample_rate / from_sample_rate
        if (has_data_file()) {

            var create_task = () => {
                var task = ShoopFileIO.load_midi_to_channels_async(
                    data_files_dir + '/' + descriptor.data_file,
                    to_sample_rate,
                    [root],
                    descriptor.n_preplay_samples,
                    descriptor.start_offset,
                    null
                    )
                task.then_delete()
                return task
            }

            if (add_tasks_to) {
                add_tasks_to.add_task(create_task())
            } else {
                registries.state_registry.set_active_io_task_fn(create_task)
            }
        }
    }
    function has_data_file() {
        return Object.keys(descriptor).includes("data_file")
    }

    function get_recorded_midi_msgs() {
        return get_midi_data().filter((msg) => msg.time >= 0)
    }

    property int initial_mode : Conversions.parse_channel_mode(descriptor.mode)
    onInitial_modeChanged: push_mode(initial_mode)
    ports_to_connect: lookup_connected_ports.objects
    property var recording_fx_chain_state_id: ('recording_fx_chain_state_id' in descriptor) ? descriptor.recording_fx_chain_state_id : null
    recording_started_at: 'recording_started_at' in descriptor ? descriptor.recording_started_at : null
    is_midi: true

    function load_data(data) { load_midi_data(data) }
    function get_data() { return get_midi_data() }

    RegistryLookups {
        id: lookup_connected_ports
        registry: registries.objects_registry
        keys: descriptor.connected_port_ids
    }

    RegisterInRegistry {
        id: reg_entry
        registry: registries.objects_registry
        key: root.descriptor.id
        object: root
    }

    Component.onCompleted: {
        push_mode(initial_mode)
    }
    function unload() {
        reg_entry.close()
        close()
        destroy()
    }
}