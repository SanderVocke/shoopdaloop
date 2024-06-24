import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6
import QtQuick.Dialogs
import ShoopDaLoop.PythonLogger
import ShoopConstants

import '../mode_helpers.js' as ModeHelpers
import '../stereo.js' as Stereo
import '../qml_url_to_filename.js' as UrlToFilename

// The loop widget allows manipulating a single loop within a track.
Item {
    id: root
    
    property var all_loops_in_track
    property var maybe_fx_chain

    property var initial_descriptor : null

    property int track_idx : -1
    property int idx_in_track : -1
    property string track_obj_id : ''

    onTrack_idxChanged: print_coords()
    onIdx_in_trackChanged: print_coords()

    function print_coords() {
        logger.debug(() => (`Loop @ (${track_idx},${idx_in_track})`))
    }

    property PythonLogger logger : PythonLogger { name: "Frontend.Qml.LoopWidget" }

    readonly property string obj_id: initial_descriptor.id
    property string name: initial_descriptor.name

    // The "loop gain" refers to the output gain from the wet
    // and/or direct channels. gain of the dry channels is always
    // at 1.
    // Furthermore, for stereo signals we resolve the individual
    // channel gains to a "gain + balance" combination for the
    // overall loop.
    readonly property real initial_gain: {
        var gains = (initial_descriptor.channels || [])
            .filter(c => ['direct', 'wet'].includes(c.mode))
            .map(c => ('gain' in c) ? c.gain : undefined)
            .filter(c => c != undefined)
        return gains.length > 0 ? Math.max(...gains) : 1.0
    }
    readonly property bool is_stereo: initial_descriptor.channels ?
        (initial_descriptor.channels.filter(c => ['direct', 'wet'].includes(c.mode) && c.type == 'audio').length == 2) :
        false
    readonly property var initial_stereo_balance : {
        if (!is_stereo) { return undefined; }
        var channels = initial_descriptor.channels.filter(c => ['direct', 'wet'].includes(c.mode) && c.type == "audio")
        channels.sort((a,b) => a.id.localeCompare(b.id))
        if (channels.length != 2) { throw new Error("Could not find stereo channels") }
        return Stereo.balance(channels[0].gain, channels[1].gain)
    }
    readonly property bool descriptor_has_audio: (initial_descriptor && !initial_descriptor.composition && initial_descriptor.channels) ?
        (initial_descriptor.channels.filter(c => c.type == "audio").length > 0) : false
    readonly property bool descriptor_has_midi: (initial_descriptor && !initial_descriptor.composition && initial_descriptor.channels) ?
        (initial_descriptor.channels.filter(c => c.type == "midi").length > 0) : false

    readonly property string object_schema : 'loop.1'
    SchemaCheck {
        descriptor: root.initial_descriptor
        schema: root.object_schema
    }

    function actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to) {
        var rval = {
            'schema': 'loop.1',
            'id': obj_id,
            'name': name,
            'length': maybe_loop ? maybe_loop.length : 0,
            'is_sync': is_sync
        }

        if (maybe_backend_loop) {
            rval['channels'] = maybe_backend_loop.channels.map((c) => c.actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to))
        } else {
            rval['channels'] = initial_descriptor.channels
            if (maybe_composite_loop) {
                rval['composition'] = maybe_composite_loop.actual_composition_descriptor()
            }
        }
        return rval
    }

    function queue_load_tasks(data_files_dir, from_sample_rate, to_sample_rate, add_tasks_to) {
        var have_data_files = initial_descriptor.channels ? initial_descriptor.channels.map(c => {
            let r = ('data_file' in c)
            if (r) { root.logger.debug(() => (`${obj_id} has data file for channel ${c.obj_id}`)) }
            else   { root.logger.debug(() => (`${obj_id} has no data file for channel ${c.obj_id}`)) }
            return r
        }) : []
        let have_any_data = have_data_files.filter(d => d == true).length > 0
        let have_composite = ('composition' in initial_descriptor)
        if (have_any_data && have_composite) {
            root.logger.error("Loop cannot both have channel data and composition information.")
            return
        } else if (have_any_data) {
            root.logger.debug(() => (`${obj_id} has data files, queueing load tasks.`))
            create_backend_loop()
            channels.forEach((c) => c.queue_load_tasks(data_files_dir, from_sample_rate, to_sample_rate, add_tasks_to))
        } else {
            root.logger.debug(() => (`${obj_id} has no data files, not queueing load tasks.`))
        }
    }

    property bool loaded : false

    property var sync_loop : registries.state_registry.sync_loop

    readonly property int cycle_length: sync_loop ? sync_loop.length : 0
    readonly property int n_cycles: cycle_length ? Math.ceil(length / cycle_length) : 0

    property var targeted_loop : registries.state_registry.targeted_loop
    property bool targeted : targeted_loop == root

    RegistryLookup {
        id: main_details_pane_lookup
        registry: registries.state_registry
        key: 'main_details_pane'
    }
    property alias main_details_pane : main_details_pane_lookup.object

    property var single_selected_composite_loop: {
        if (selected_loops && selected_loops.size == 1) {
            let selected_loop = Array.from(selected_loops)[0]
            if (selected_loop.maybe_composite_loop) {
                return selected_loop
            }
        }
        return null
    }
    property var loops_in_single_selected_composite_loop: (single_selected_composite_loop &&
        single_selected_composite_loop.maybe_composite_loop &&
        single_selected_composite_loop.maybe_composite_loop.all_loops) ? single_selected_composite_loop.maybe_composite_loop.all_loops : new Set()
    property bool is_in_selected_composite_loop : loops_in_single_selected_composite_loop.has(root)

    SelectedLoops { id: selected_loops_lookup }
    property alias selected_loops : selected_loops_lookup.loops
    property bool selected : selected_loops.has(this)

    RegisterInRegistry {
        id: obj_reg_entry
        object: root
        key: obj_id
        registry: registries.objects_registry
    }

    RegisterInRegistry {
        id: sync_reg_entry
        enabled: initial_descriptor.is_sync
        registry: registries.state_registry
        key: 'sync_loop'
        object: root
    }

    function init() {
        if (is_sync) { 
            root.logger.debug("Initializing back-end for sync loop")
            create_backend_loop()
        } else if (initial_descriptor && 'composition' in initial_descriptor) {
            root.logger.debug(() => (`${obj_id} has composition, creating composite loop.`))
            create_composite_loop(initial_descriptor.composition)
        } 
        loaded = true
    }

    Component.onCompleted: init()
    onInitial_descriptorChanged: init()

    property var additional_context_menu_options : null // dict of option name -> functor

    onIs_loadedChanged: if(is_loaded) { root.logger.debug(() => ("Loaded back-end loop.")) }
    onIs_syncChanged: if(is_sync) { create_backend_loop() }

    // Internally controlled
    property var maybe_loop : null
    readonly property var maybe_backend_loop : (maybe_loop && maybe_loop instanceof Loop) ? maybe_loop : null
    readonly property var maybe_composite_loop : (maybe_loop && maybe_loop instanceof CompositeLoop) ? maybe_loop : null
    readonly property bool is_loaded : maybe_loop
    readonly property bool is_sync: sync_loop && sync_loop == this
    readonly property bool is_script: maybe_composite_loop && maybe_composite_loop.kind == 'script'
    readonly property var delay_for_targeted : {
        // This property is used for synchronizing to the targeted loop.
        // If:
        // - A targeted loop is active, and
        // - The targeted loop is doing playback
        // then this property will hold the amount of sync loop cycles
        // to delay in order to transition in sync with the targeted loop.
        // Otherwise, it will be 0.
        if (targeted_loop && sync_loop) {
            var to_transition = undefined
            if (targeted_loop.next_transition_delay >= 0 && targeted_loop.next_mode >= 0) {
                to_transition = targeted_loop.next_transition_delay
            }
            var to_end = undefined
            if (ModeHelpers.is_mode_with_predictable_end(targeted_loop.mode)) {
                to_end = Math.floor((targeted_loop.length - targeted_loop.position) / sync_loop.length)
            }
            if (to_transition == undefined && to_end == undefined) { return undefined }
            else if (to_transition != undefined && to_end != undefined) { return Math.min(to_transition, to_end) }
            else if (to_transition != undefined) { return to_transition }
            else return to_end
        }
        return undefined
    }
    readonly property int use_delay : delay_for_targeted != undefined ? delay_for_targeted : 0

    property int n_multiples_of_sync_length: (sync_loop && maybe_loop) ?
        Math.ceil(maybe_loop.length / sync_loop.length) : 1
    property int current_cycle: (sync_loop && maybe_loop) ?
        Math.floor(maybe_loop.position / sync_loop.length) : 0

    // possible values: "with_targeted", "infinite", or int > 0
    readonly property var record_kind : {
        if (root.is_sync) { return "infinite" }
        if (root.delay_for_targeted != undefined) { return "with_targeted" }
        let apply_n_cycles = registries.state_registry.apply_n_cycles
        if (apply_n_cycles <= 0) { return "infinite" }
        return apply_n_cycles
    }

    // Signals
    signal cycled(int cycle_nr)

    // Methods
    function transition_loops(loops, mode, maybe_delay, maybe_align_to_sync_at) {
        loops.forEach(loop => {
            // Force to load all loops involved
            loop.create_backend_loop()
        })
        var backend_loops = loops.filter(o => o.maybe_backend_loop).map(o => o.maybe_backend_loop)
        var other_loops = loops.filter(o => o.maybe_loop && !o.maybe_backend_loop).map(o => o.maybe_loop)
        if (backend_loops.length > 0) { backend_loops[0].transition_multiple(backend_loops, mode, maybe_delay, maybe_align_to_sync_at) }
        other_loops.forEach(o => o.transition(mode, maybe_delay, maybe_align_to_sync_at))
    }
    function transition(mode, maybe_delay, maybe_align_to_sync_at, include_selected=true) {
        // Do the transition for this loop and all selected loops, if any
        var selected_all = include_selected ? selected_loops : new Set()
        
        if (selected_all.has (root)) {
            // If we are part of the selection, transition them all as a group.
            var objects = Array.from(selected_all)
            transition_loops(objects, mode, maybe_delay, maybe_align_to_sync_at)
        } else {
            // If we are not part of the selection, transition ourselves only.
            transition_loops([root], mode, maybe_delay, maybe_align_to_sync_at)
        }        
    }
    function trigger_mode_button(mode) {
        if (mode == ShoopConstants.LoopMode.Stopped) { root.on_stop_clicked() }
        else if (mode == ShoopConstants.LoopMode.Playing) { root.on_play_clicked() }
        else if (mode == ShoopConstants.LoopMode.PlayingDryThroughWet) { root.on_playdry_clicked() }
        else if (mode == ShoopConstants.LoopMode.Recording) { root.on_record_clicked() }
        else if (mode == ShoopConstants.LoopMode.RecordingDryIntoWet) { root.on_recordfx_clicked() }
    }

    function selected_and_other_loops_in_track() {
        // Gather all selected loops
        var selected_all = selected_loops
        selected_all.add(root)
        // Gather all other loops that are in the same track(s)
        var _all_track_loops = []
        selected_all.forEach(loop => {
            for(var i=0; i<loop.all_loops_in_track.length; i++) {
                _all_track_loops.push(loop.all_loops_in_track[i])
            }
        })
        var _other_loops = _all_track_loops.filter(l => !selected_all.has(l))

        return [Array.from(selected_all), _other_loops]
    }

    // Apply the given transition to the currently selected loop(s),
    // and stop all other loops in the same track(s)
    function transition_solo_in_track(mode, maybe_delay, maybe_align_to_sync_at) {
        let r = selected_and_other_loops_in_track()
        // Do the transitions
        transition_loops(r[1], ShoopConstants.LoopMode.Stopped, maybe_delay, maybe_align_to_sync_at)
        transition_loops(r[0], mode, maybe_delay, maybe_align_to_sync_at)
    }

    function clear(length=0, emit=true) {
        if(maybe_loop) {
            maybe_loop.clear(length);
            if (maybe_composite_loop) {
                maybe_loop.qml_close()
                maybe_loop.destroy(30)
                maybe_loop.parent = null
                maybe_loop = null
            }
        }
    }
    function qml_close() {
        obj_reg_entry.close()
        sync_reg_entry.close()
        if (maybe_loop) {
            maybe_loop.qml_close()
            maybe_loop.destroy(30)
            maybe_loop.parent = null
            maybe_loop = null
        }
    }

    function select(clear = false) {
        untarget()
        if (!clear) {
            registries.state_registry.add_to_set('selected_loop_ids', obj_id)
        } else {
            registries.state_registry.replace('selected_loop_ids', new Set([obj_id]))
        }
    }
    function deselect(clear = false) {
        if (!clear) {
            registries.state_registry.remove_from_set('selected_loop_ids', obj_id)
        } else {
            registries.state_registry.replace('selected_loop_ids', new Set())
        }
    }
    function target() {
        deselect()
        registries.state_registry.set_targeted_loop(root)
    }
    function untarget() {
        if (targeted) {
            registries.state_registry.untarget_loop()
        }
    }
    function toggle_selected(clear_if_select = false) {
        if (selected) { deselect() } else { select(clear_if_select) }
    }
    function toggle_targeted() {
        if (targeted) { untarget() } else { target() }
    }

    function get_audio_channels() {
        return maybe_loop ? maybe_loop.get_audio_channels() : []
    }

    function get_midi_channels() {
        return maybe_loop ? maybe_loop.get_midi_channels() : []
    }

    function get_audio_output_channels() {
        return get_audio_channels().filter(c => c && c.obj_id.match(/.*_(?:wet|direct)(?:_[0-9]+)?$/))
    }

    function get_midi_output_channels() {
        return get_midi_channels().filter(c => c && c.obj_id.match(/.*_(?:wet|direct)(?:_[0-9]+)?$/))
    }

    function get_stereo_audio_output_channels() {
        var chans = get_audio_output_channels()
        if (chans.length != 2) { throw new Error("attempting to get stereo channels, no stereo found") }
        chans.sort((a, b) => a.obj_id.localeCompare(b.obj_id))
        return chans
    }

    function set_gain_fader(value) {
        statusrect.gain_dial.set_as_range_fraction(value)
    }
    function get_gain_fader() {
        return statusrect.gain_dial.position
    }

    property real last_pushed_gain: initial_gain
    property real last_pushed_stereo_balance: initial_stereo_balance ? initial_stereo_balance : 0.0

    function push_gain(gain) {
        // Only set the gain on audio output channels:
        // - gain not supported on MIDI
        // - Send should always have the original recorded gain of the dry signal.
        // Also, gain + balance together make up the channel gains in stereo mode.
        if (root.is_stereo && root.maybe_backend_loop) {
            root.logger.debug(`push stereo gain: ${gain}`)
            var lr = get_stereo_audio_output_channels()
            var gains = Stereo.individual_gains(gain, last_pushed_stereo_balance)
            lr[0].set_gain(gains[0])
            lr[1].set_gain(gains[1])
        } else {
            root.logger.debug(`push homogenous gain: ${gain}`)
            get_audio_output_channels().forEach(c => c.set_gain(gain))
        }

        last_pushed_gain = gain
    }

    function push_stereo_balance(balance) {
        if (root.is_stereo && root.maybe_backend_loop) {
            root.logger.debug(`push balance: ${balance}`)
            var lr = get_stereo_audio_output_channels()
            var gains = Stereo.individual_gains(last_pushed_gain, balance)
            lr[0].set_gain(gains[0])
            lr[1].set_gain(gains[1])
        }
        last_pushed_stereo_balance = balance
    }

    function set_length(length) {
        maybe_loop && maybe_loop.set_length(length)
    }

    function record_n(delay_start, n) {
        if (registries.state_registry.solo_active) {
            root.transition_solo_in_track(ShoopConstants.LoopMode.Recording, delay_start, ShoopConstants.DontAlignToSyncImmediately)
            root.transition(
                registries.state_registry.play_after_record_active ? ShoopConstants.LoopMode.Playing : ShoopConstants.LoopMode.Stopped,
                delay_start + n,
                ShoopConstants.DontAlignToSyncImmediately
            )
        } else {
            root.transition(ShoopConstants.LoopMode.Recording, delay_start, ShoopConstants.DontAlignToSyncImmediately)
            root.transition(
                registries.state_registry.play_after_record_active ? ShoopConstants.LoopMode.Playing : ShoopConstants.LoopMode.Stopped,
                delay_start + n,
                ShoopConstants.DontAlignToSyncImmediately
            )
        }
    }

    function record_with_targeted() {
        if (!root.targeted_loop) { return }
        // A target loop is set. Do the "record together with" functionality.
        // TODO: code is duplicated in app shared state for MIDI source
        var n_cycles_delay = 0
        var n_cycles_record = 1
        n_cycles_record = Math.ceil(root.targeted_loop.length / root.sync_loop.length)
        if (ModeHelpers.is_playing_mode(root.targeted_loop.mode)) {
            var current_cycle = Math.floor(root.targeted_loop.position / root.sync_loop.length)
            n_cycles_delay = Math.max(0, n_cycles_record - current_cycle - 1)
        }
        root.record_n(n_cycles_delay, n_cycles_record)
    }

    function adopt_ringbuffers(reverse_start_cycle, cycles_length, go_to_cycle, go_to_mode) {
        if (!root.maybe_loop) {
            create_backend_loop()
        }
        maybe_loop.adopt_ringbuffers(reverse_start_cycle, cycles_length, go_to_cycle, go_to_mode)
    }

    anchors {
        left: parent ? parent.left : undefined
        right: parent ? parent.right : undefined
        leftMargin: 2
        rightMargin: 2
    }
    height: childrenRect.height
    clip: true

    readonly property int length : maybe_loop ? maybe_loop.length : 0
    readonly property int position : maybe_loop ? maybe_loop.position : 0
    readonly property int mode : maybe_loop ? maybe_loop.mode : ShoopConstants.LoopMode.Stopped
    readonly property int next_mode : maybe_loop ? maybe_loop.next_mode : ShoopConstants.LoopMode.Stopped
    readonly property int next_transition_delay : maybe_loop ? maybe_loop.next_transition_delay : -1
    
    Component {
        id: backend_loop_factory
        BackendLoopWithChannels {
            maybe_fx_chain: root.maybe_fx_chain
            loop_widget: root
        }
    }

    function create_backend_loop() {
        if (!maybe_loop) {
            if (backend_loop_factory.status == Component.Error) {
                throw new Error("BackendLoopWithChannels: Failed to load factory: " + backend_loop_factory.errorString())
            } else if (backend_loop_factory.status != Component.Ready) {
                throw new Error("BackendLoopWithChannels: Factory not ready: " + backend_loop_factory.status.toString())
            } else {
                let gain = last_pushed_gain
                let balance = last_pushed_stereo_balance
                maybe_loop = backend_loop_factory.createObject(root, {
                    'initial_descriptor': root.initial_descriptor,
                    'sync_source': Qt.binding(() => (!is_sync && root.sync_loop && root.sync_loop.maybe_backend_loop) ? root.sync_loop.maybe_backend_loop : null),
                })
                push_stereo_balance(balance)
                push_gain(gain)
                maybe_loop.onCycled.connect(root.cycled)
            }
        }
    }

    Component {
        id: composite_loop_factory
        CompositeLoop {
            loop_widget: root
        }
    }
    function create_composite_loop(composition={
        'playlists': []
    }) {
        if (maybe_backend_loop) {
            if (!is_sync && maybe_backend_loop.is_all_empty()) {
                // Empty backend loop can be converted to composite loop.
                maybe_loop.qml_close()
                maybe_loop.destroy(30)
                maybe_loop.parent = null
                maybe_loop = null
            } else {
                root.logger.error("Non-empty or sync loop cannot be converted to composite")
                return
            }
        }
        if (maybe_loop) {
            return
        }
        if (composite_loop_factory.status == Component.Error) {
            throw new Error("CompositeLoop: Failed to load factory: " + composite_loop_factory.errorString())
        } else if (composite_loop_factory.status != Component.Ready) {
            throw new Error("CompositeLoop: Factory not ready: " + composite_loop_factory.status.toString())
        } else {
            maybe_loop = composite_loop_factory.createObject(root, {
                initial_composition_descriptor: composition,
                obj_id: root.obj_id
            })
            maybe_loop.onCycled.connect(root.cycled)
        }
    }

    function on_play_clicked() {
        if (registries.state_registry.solo_active) {
            root.transition_solo_in_track(ShoopConstants.LoopMode.Playing,
                root.sync_active ? root.use_delay : ShoopConstants.DontWaitForSync,
                ShoopConstants.DontAlignToSyncImmediately)
        } else {
            root.transition(ShoopConstants.LoopMode.Playing,
                root.sync_active ? root.use_delay : ShoopConstants.DontWaitForSync,
                ShoopConstants.DontAlignToSyncImmediately)
        }
    }

    function on_playdry_clicked() {
        if (registries.state_registry.solo_active) {
            root.transition_solo_in_track(ShoopConstants.LoopMode.PlayingDryThroughWet,
                root.sync_active ? root.use_delay : ShoopConstants.DontWaitForSync,
                ShoopConstants.DontAlignToSyncImmediately)
        } else {
            root.transition(ShoopConstants.LoopMode.PlayingDryThroughWet,
                root.sync_active ? root.use_delay : ShoopConstants.DontWaitForSync,
                ShoopConstants.DontAlignToSyncImmediately)
        }
    }

    function on_record_clicked() {
        if (root.record_kind == 'infinite' || root.maybe_composite_loop) {
            if (registries.state_registry.solo_active) {
                root.transition_solo_in_track(ShoopConstants.LoopMode.Recording,
                    root.sync_active ? root.use_delay : ShoopConstants.DontWaitForSync,
                    ShoopConstants.DontAlignToSyncImmediately)
            } else {
                root.transition(ShoopConstants.LoopMode.Recording,
                    root.sync_active ? root.use_delay : ShoopConstants.DontWaitForSync,
                    ShoopConstants.DontAlignToSyncImmediately)
            }
        } else if (root.record_kind == 'with_targeted') {
            root.record_with_targeted();
        } else {
            root.record_n(0, root.record_kind)
        } 
    }

    readonly property int n_cycles_to_grab : {
        var rval = registries.state_registry.apply_n_cycles
        if (rval <= 0) { rval = 1; }
        return rval
    }

    function on_grab_clicked() {
        let selection = selected_loops.has(root) ? selected_loops : [root]

        if (!root.maybe_loop) {
            root.create_backend_loop()
        }
        if (root.sync_active) {
            let go_to_mode = registries.state_registry.play_after_record_active ? ShoopConstants.LoopMode.Playing : ShoopConstants.LoopMode.Unknown
            if (root.targeted_loop) {
                // Grab and sync up with the running targeted loop
                selection.forEach(l => l.adopt_ringbuffers(root.targeted_loop.current_cycle + root.targeted_loop.n_cycles, root.targeted_loop.n_cycles,
                    root.targeted_loop.current_cycle, go_to_mode))
            } else {
                selection.forEach(l => l.adopt_ringbuffers(root.n_cycles_to_grab, root.n_cycles_to_grab, 0, go_to_mode))
            }
        } else {
            if (root.targeted_loop) {
                // Grab current targeted loop content and record the rest
                root.adopt_ringbuffers(null, root.targeted_loop.current_cycle + 1, root.targeted_loop.current_cycle, ShoopConstants.LoopMode.Recording)
                root.transition(
                    registries.state_registry.play_after_record_active ? ShoopConstants.LoopMode.Playing : ShoopConstants.LoopMode.Stopped,
                    root.delay_for_targeted,
                    ShoopConstants.DontAlignToSyncImmediately
                )
            } else {
                let goto_cycle =
                    root.maybe_composite_loop ?
                        root.maybe_composite_loop.n_cycles - 1 :
                        root.n_cycles_to_grab - 1
                root.adopt_ringbuffers(null, root.n_cycles_to_grab, goto_cycle, ShoopConstants.LoopMode.Recording)
                root.transition(
                    registries.state_registry.play_after_record_active ? ShoopConstants.LoopMode.Playing : ShoopConstants.LoopMode.Stopped,
                    0,
                    ShoopConstants.DontAlignToSyncImmediately
                )
            }
        }

        if (registries.state_registry.solo_active) {
            let r = selected_and_other_loops_in_track()
            root.transition_loops(r[1], ShoopConstants.LoopMode.Stopped, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
        }
    }

    function on_stop_clicked() {
        root.transition(
           ShoopConstants.LoopMode.Stopped,
           root.sync_active ? root.use_delay : ShoopConstants.DontWaitForSync,
           ShoopConstants.DontAlignToSyncImmediately)
    }

    function on_recordfx_clicked() {
        var n = root.n_multiples_of_sync_length
        var delay = 
            root.delay_for_targeted != undefined ? 
                root.use_delay : // delay to other
                root.n_multiples_of_sync_length - root.current_cycle - 1 // delay to self
        var prev_mode = statusrect.loop.mode
        root.transition(ShoopConstants.LoopMode.RecordingDryIntoWet, delay, ShoopConstants.DontAlignToSyncImmediately)
        statusrect.loop.transition(prev_mode, delay + n, ShoopConstants.DontAlignToSyncImmediately)
    }

    property bool initialized : maybe_loop ? (maybe_loop.initialized ? true : false) : false
    property var channels: (maybe_loop && maybe_loop.channels) ? maybe_loop.channels : []
    property var audio_channels : (maybe_loop && maybe_loop.audio_channels) ? maybe_loop.audio_channels : []
    property var midi_channels : (maybe_loop && maybe_loop.midi_channels) ? maybe_loop.midi_channels : []
   
    property bool sync_active : registries.state_registry.sync_active

    // UI
    StatusRect {
        id: statusrect
        loop: root.maybe_loop

        anchors {
            left: parent.left
            right: parent.right
        }

        ContextMenu {
            id: contextmenu
        }
    }

    component StatusRect : Rectangle {
        id: statusrect
        property var loop
        property bool hovered : area.containsMouse
        property string name : root.name

        property alias gain_dial: gain_dial

        signal propagateMousePosition(var point)
        signal propagateMouseExited()

        anchors {
            left: parent.left
            right: parent.right
        }
        height: 26

        color: {
            if (loop && root.maybe_composite_loop && root.maybe_composite_loop.kind == 'regular') {
                return 'pink'
            } else if (loop && root.maybe_composite_loop && root.maybe_composite_loop.kind == 'script') {
                return '#77AA77'
            } else if (loop && loop.length > 0) {
                return '#000044'
            }
            return Material.background
        }

        border.color: {
            var default_color = 'grey'

            if (root.targeted) {
                return "orange";
            } else if (root.selected) {
                return 'yellow';
            } else if (root.is_in_selected_composite_loop && root.single_selected_composite_loop.maybe_composite_loop.kind == 'regular') {
                return 'pink';
            } else if (root.is_in_selected_composite_loop && root.single_selected_composite_loop.maybe_composite_loop.kind == 'script') {
                return '#77AA77'
            }

            if (!statusrect.loop || statusrect.loop.length == 0) {
                return default_color;
            }

            return '#DDDDDD' //blue'
        }
        border.width: 2

        Item {
            anchors.fill: parent
            anchors.margins: 2

            LoopProgressRect {
                anchors.fill: parent
                loop: statusrect.loop
            }
        }

        ProgressBar {
            id: peak_meter_l
            visible: root.is_stereo
            anchors {
                left: parent.left
                right: parent.horizontalCenter
                bottom: parent.bottom
                margins: 2
            }
            height: 3

            AudioLevelMeterModel {
                id: output_peak_meter_l
                max_dt: 0.1
                input: (root.maybe_loop && root.maybe_loop.display_peaks && root.maybe_loop.display_peaks.length >= 1) ? root.maybe_loop.display_peaks[0] : 0.0
            }

            from: -50.0
            to: 0.0
            value: output_peak_meter_l.value

            background: Item { anchors.fill: peak_meter_l }
            contentItem: Item {
                implicitWidth: peak_meter_l.width
                implicitHeight: peak_meter_l.height

                Rectangle {
                    width: peak_meter_l.visualPosition * peak_meter_l.width
                    height: peak_meter_l.height
                    color: Material.accent
                    x: peak_meter_l.width - width
                }
            }
        }

        ProgressBar {
            id: peak_meter_r
            visible: root.is_stereo
            anchors {
                left: parent.horizontalCenter
                right: parent.right
                bottom: parent.bottom
                margins: 2
            }
            height: 3

            AudioLevelMeterModel {
                id: output_peak_meter_r
                max_dt: 0.1
                input: (root.maybe_loop && root.maybe_loop.display_peaks && root.maybe_loop.display_peaks.length >= 2) ? root.maybe_loop.display_peaks[1] : 0.0
            }

            from: -50.0
            to: 0.0
            value: output_peak_meter_r.value

            background: Item { anchors.fill: peak_meter_r }
            contentItem: Item {
                implicitWidth: peak_meter_r.width
                implicitHeight: peak_meter_r.height

                Rectangle {
                    width: peak_meter_r.visualPosition * peak_meter_r.width
                    height: peak_meter_r.height
                    color: Material.accent
                }
            }
        }

        ProgressBar {
            id: peak_meter_overall
            visible: !root.is_stereo
            anchors {
                left: peak_meter_l.left
                right: peak_meter_r.right
                bottom: peak_meter_l.bottom
                top: peak_meter_l.top
            }
            height: 3

            AudioLevelMeterModel {
                id: output_peak_meter_overall
                max_dt: 0.1
                input: (root.maybe_loop && root.maybe_loop.display_peaks && root.maybe_loop.display_peaks.length > 0) ? Math.max(...root.maybe_loop.display_peaks) : 0.0
            }

            from: -30.0
            to: 0.0
            value: output_peak_meter_overall.value

            background: Item { anchors.fill: peak_meter_overall }
            contentItem: Item {
                implicitWidth: peak_meter_overall.width
                implicitHeight: peak_meter_overall.height

                Rectangle {
                    width: peak_meter_overall.visualPosition * peak_meter_overall.width
                    height: peak_meter_overall.height
                    color: Material.accent
                }
            }
        }

        MaterialDesignIcon {
            size: 10
            name: 'star'
            color: "yellow"

            anchors {
                left: parent.left
                leftMargin: 2
                top: parent.top
                topMargin: 2
            }

            visible: root.is_sync
        }

        Item {
            id: loopitem
            anchors.fill: parent

            Item {
                id: iconitem
                width: 24
                height: 24
                y: 0
                x: 0

                property bool show_next_mode: 
                    statusrect.loop &&
                        statusrect.loop.next_mode != null &&
                        statusrect.loop.next_transition_delay != null &&
                        statusrect.loop.next_transition_delay >= 0 &&
                        statusrect.loop.mode != statusrect.loop.next_mode

                LoopStateIcon {
                    id: loopstateicon
                    mode: statusrect.loop ? statusrect.loop.mode : ShoopConstants.LoopMode.Unknown
                    show_timer_instead: parent.show_next_mode
                    visible: !parent.show_next_mode || (parent.show_next_mode && statusrect.loop.next_transition_delay == 0)
                    connected: true
                    is_regular_composite: root.maybe_composite_loop ? root.maybe_composite_loop.kind == 'regular' : false
                    is_script_composite: root.maybe_composite_loop ? root.maybe_composite_loop.kind == 'script' : false
                    size: iconitem.height
                    y: 0
                    anchors.horizontalCenter: iconitem.horizontalCenter
                    muted: false
                    empty: !statusrect.loop || statusrect.loop.length == 0
                    onDoubleClicked: (event) => {
                            if (!key_modifiers.alt_pressed && event.button === Qt.LeftButton) { root.target() }
                        }
                    onClicked: (event) => {
                        if (event.button === Qt.LeftButton) { 
                            if (key_modifiers.alt_pressed) {
                                if (root.selected_loops.size == 1) {
                                    let selected = Array.from(root.selected_loops)[0]
                                    if (selected != root) {
                                        selected.create_composite_loop()
                                        if (selected.maybe_composite_loop) {
                                            // Add the selected loop to the currently selected composite loop.
                                            // If ctrl pressed, as a new parallel timeline; otherwise at the end of the default timeline.
                                            if (key_modifiers.control_pressed) {
                                                selected.maybe_composite_loop.add_loop(root, 0, registries.state_registry.apply_n_cycles, undefined)
                                            } else {
                                                selected.maybe_composite_loop.add_loop(root, 0, registries.state_registry.apply_n_cycles, 0)
                                            }
                                        }
                                    }
                                }
                            } else if (root.targeted) { root.untarget(); root.deselect() }
                            else { root.toggle_selected(!key_modifiers.control_pressed) }
                        }
                        else if (event.button === Qt.RightButton) { contextmenu.popup() }
                    }
                }
                LoopStateIcon {
                    id: loopnextstateicon
                    mode: parent.show_next_mode ?
                        statusrect.loop.next_mode : ShoopConstants.LoopMode.Unknown
                    show_timer_instead: false
                    is_regular_composite: false
                    is_script_composite: false
                    connected: true
                    size: iconitem.height * 0.65
                    y: 0
                    anchors.right : loopstateicon.right
                    anchors.bottom : loopstateicon.bottom
                    visible: parent.show_next_mode
                    onClicked: (event) => loopstateicon.clicked(event)
                }
                Text {
                    text: statusrect.loop ? (statusrect.loop.next_transition_delay + 1).toString(): ''
                    visible: parent.show_next_mode && statusrect.loop.next_transition_delay > 0
                    anchors.fill: loopstateicon
                    color: Material.foreground
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                    font.bold: true
                }
            }

            Text {
                text: statusrect.name
                color: /\([0-9]+\)/.test(statusrect.name) ? 'grey' : Material.foreground
                font.pixelSize: 11
                verticalAlignment: Text.AlignVCenter
                horizontalAlignment: Text.AlignLeft
                anchors.left: iconitem.right
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                anchors.right: parent.right
                anchors.rightMargin: 6
                visible: !buttongrid.visible
            }

            // Draggy rect for moving the track
            Rectangle {
                id: mover
                anchors.fill: parent

                // for debugging
                // color: 'yellow'
                color: 'transparent'

                Item {
                    id: movable
                    width: mover.width
                    height: mover.height
                    parent: Overlay.overlay
                    visible: area.drag.active
                    z: 3

                    Drag.active: area.drag.active
                    Drag.hotSpot.x : width/2
                    Drag.hotSpot.y : height/2
                    Drag.source: root
                    Drag.keys: ['LoopWidget', 'LoopWidget_track_' + root.track_obj_id]

                    Rectangle {
                        anchors.fill: parent
                        color: 'white'
                        opacity: 0.5

                        Image {
                            id: movable_image
                            source: ''
                        }
                    }

                    function resetCoords() {
                        x = mover.mapToItem(Overlay.overlay, 0, 0).x
                        y = mover.mapToItem(Overlay.overlay, 0, 0).y
                    }
                    Component.onCompleted: resetCoords()
                    onVisibleChanged: resetCoords()
                }
            }

            MouseArea {
                id: area
                x: 0
                y: 0
                anchors.fill: parent
                hoverEnabled: true
                propagateComposedEvents: true
                onPositionChanged: (mouse) => { statusrect.propagateMousePosition(mapToGlobal(mouse.x, mouse.y)) }
                onExited: statusrect.propagateMouseExited()

                cursorShape: Qt.PointingHandCursor

                drag {
                    target: movable
                    onActiveChanged: {
                        if (active) {
                            root.grabToImage((result) => {
                                movable_image.source = result.url
                            })
                        }
                    }
                }

                onReleased: movable.Drag.drop()
                onPressed: movable.resetCoords()
            }

            Grid {
                visible: statusrect.hovered || playlivefx.hovered || record_grab.hovered || recordfx.hovered
                x: 20
                y: 2
                columns: 4
                id: buttongrid
                property int button_width: 18
                property int button_height: 22
                spacing: 1

                SmallButtonWithCustomHover {
                    id : play
                    width: buttongrid.button_width
                    height: buttongrid.button_height

                    IconWithText {
                        size: parent.width
                        anchors.centerIn: parent
                        name: 'play'
                        color: root.is_script ? 'white' : 'green'
                        text_color: Material.foreground
                        text: {
                            var rval = ''
                            if (root.delay_for_targeted != undefined)  { rval += '>' }
                            if (registries.state_registry.solo_active) { rval += 'S' }
                            return rval
                        }
                        font.pixelSize: size / 2.0
                    }

                    onClicked: root.on_play_clicked()

                    ToolTip.delay: 1000
                    ToolTip.timeout: 5000
                    ToolTip.visible: hovered
                    ToolTip.text: 'Play wet recording' +
                        (registries.state_registry.sync_active ? ' (synchronous)' : ' (immediate)') +
                        (registries.state_registry.solo_active ? ' (solo in track)' : '') +
                        (root.delay_for_targeted != undefined  ? ' (with targeted loop)' : '')
                        + '.'

                    Connections {
                        target: statusrect
                        function onPropagateMousePosition(pt) { play.onMousePosition(pt) }
                        function onPropagateMouseExited() { play.onMouseExited() }
                    }

                    Popup {
                        background: Item{}
                        visible: !root.is_script && (play.hovered || ma.containsMouse)
                        leftInset: 0
                        rightInset: 0
                        topInset: 0
                        bottomInset: 0
                        padding: 0
                        margins: 0

                        x: 0
                        y: play.height

                        Rectangle {
                            width: playlivefx.width
                            height: playlivefx.height
                            color: statusrect.color
                            clip: true

                            MouseArea {
                                id: ma
                                x: 0
                                y: 0
                                width: parent.width
                                height: parent.height
                                hoverEnabled: true

                                onPositionChanged: (mouse) => { 
                                    var p = mapToGlobal(mouse.x, mouse.y)
                                    playlivefx.onMousePosition(p)
                                }
                                onExited: { playlivefx.onMouseExited() }
                            }

                            Column {
                                SmallButtonWithCustomHover {
                                    id : playlivefx
                                    width: buttongrid.button_width
                                    height: buttongrid.button_height
                                    
                                    IconWithText {
                                        size: parent.width
                                        anchors.centerIn: parent
                                        name: 'play'
                                        color: 'orange'
                                        text_color: Material.foreground
                                        text: {
                                            var rval = ''
                                            if (root.delay_for_targeted != undefined)  { rval += '>' }
                                            if (registries.state_registry.solo_active) { rval += 'S' }
                                            return rval
                                        }
                                        font.pixelSize: size / 2.0
                                    }

                                    onClicked: root.on_playdry_clicked()

                                    ToolTip.delay: 1000
                                    ToolTip.timeout: 5000
                                    ToolTip.visible: hovered
                                    ToolTip.text: "Play dry recording through live effects" +
                                        (registries.state_registry.sync_active ? ' (synchronous)' : ' (immediate)') +
                                        (registries.state_registry.solo_active ? ' (solo in track)' : '') +
                                        (root.delay_for_targeted != undefined  ? ' (with targeted loop)' : '') +
                                        + '.'
                                }
                            }
                        }
                    }
                }

                SmallButtonWithCustomHover {
                    id : record
                    width: buttongrid.button_width
                    height: buttongrid.button_height

                    visible: !root.is_script

                    IconWithText {
                        id: record_icon
                        size: parent.width
                        anchors.centerIn: parent
                        name: 'record'
                        color: 'red'
                        text_color: Material.foreground
                        text: {
                            var rval = root.record_kind == 'with_targeted' ? '><' :
                                       root.record_kind == 'infinite' ? '' :
                                       root.record_kind.toString() // integer
                            if (registries.state_registry.solo_active) { rval += 'S' }
                            return rval
                        }
                        font.pixelSize: size / 2.0

                        // If play-after-record is active, render a half-green icon
                        Item {
                            //clipping box that cuts the underlying icon
                            anchors.fill: parent
                            anchors.leftMargin: parent.width / 2 + 1
                            clip: true

                            MaterialDesignIcon {
                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                size: record_icon.size
                                name: 'record'
                                color: 'green'
                                visible: registries.state_registry.play_after_record_active
                            }
                        }
                    }

                    onClicked: root.on_record_clicked()

                    ToolTip.delay: 1000
                    ToolTip.timeout: 5000
                    ToolTip.visible: hovered
                    ToolTip.text: "Record" +
                                        (registries.state_registry.play_after_record_active ? ', then play' : ', then stop') +
                                        (root.record_kind == 'infinite' ? ' (until stopped)' : (root.record_kind == 'with_targeted' ? ' (with targeted loop)' : ` (${root.record_kind} cycles)`)) +
                                        (registries.state_registry.sync_active ? ' (synchronous)' : ' (immediate)') +
                                        (registries.state_registry.solo_active ? ' (solo in track)' : '')
                                        + '.'
                    Connections {
                        target: statusrect
                        function onPropagateMousePosition(pt) { record.onMousePosition(pt) }
                        function onPropagateMouseExited() { record.onMouseExited() }
                    }

                    Popup {
                        background: Item{}
                        visible: !root.is_script && (record.hovered || ma_.containsMouse)
                        leftInset: 0
                        rightInset: 0
                        topInset: 0
                        bottomInset: 0
                        padding: 0
                        margins: 0

                        x: 0
                        y: play.height

                        Rectangle {
                            width: record_grab.width
                            height: (record_grab.visible ? record_grab.height : 0) + recordfx.height
                            color: statusrect.color

                            MouseArea {
                                id: ma_
                                x: 0
                                y: 0
                                width: parent.width
                                height: parent.height
                                hoverEnabled: true

                                onPositionChanged: (mouse) => { 
                                    var p = mapToGlobal(mouse.x, mouse.y)
                                    record_grab.onMousePosition(p)
                                    recordfx.onMousePosition(p)
                                }
                                onExited: { record_grab.onMouseExited(); recordfx.onMouseExited() }
                            }

                            Column {
                                SmallButtonWithCustomHover {
                                    id : record_grab
                                    
                                    // This feature makes no sense for composite script loops,
                                    // which cannot be treated as a "loop".
                                    // But for regular composite loops, it makes sense - grab
                                    // each portion to the correct subloop.
                                    visible: {
                                        if (root.maybe_composite_loop) {
                                            return root.maybe_composite_loop.kind == 'regular'
                                        }
                                        return true;
                                    }
                                    width: buttongrid.button_width
                                    height: buttongrid.button_height

                                    IconWithText {
                                        size: parent.width
                                        anchors.centerIn: parent
                                        name: 'arrow-collapse-down'
                                        color: 'red'
                                        text_color: Material.foreground
                                        text: root.n_cycles_to_grab.toString()
                                        font.pixelSize: size / 2.0

                                        // If play-after-record is active, render a half-green icon
                                        Item {
                                            //clipping box that cuts the underlying icon
                                            anchors.fill: parent
                                            anchors.topMargin: parent.height / 2 
                                            clip: true

                                            MaterialDesignIcon {
                                                anchors.bottom: parent.bottom
                                                anchors.horizontalCenter: parent.horizontalCenter
                                                size: record_icon.size
                                                name: 'arrow-collapse-down'
                                                color: 'green'
                                                visible: registries.state_registry.play_after_record_active
                                            }
                                        }
                                    }

                                    onClicked: root.on_grab_clicked()

                                    ToolTip.delay: 1000
                                    ToolTip.timeout: 5000
                                    ToolTip.visible: hovered
                                    ToolTip.text: "Grab always-on recording" +
                                        (registries.state_registry.play_after_record_active ? ' and play immediately' : '') +
                                        ((registries.state_registry.solo_active && registries.state_registry.play_after_record_active) ? ' (solo in track)' : '')
                                        + '.'
                                }
                            
                                SmallButtonWithCustomHover {
                                    id : recordfx
                                    width: buttongrid.button_width
                                    height: buttongrid.button_height
                                    IconWithText {
                                        size: parent.width
                                        anchors.centerIn: parent
                                        name: 'record'
                                        color: 'orange'
                                        text_color: Material.foreground
                                        text: root.delay_for_targeted != undefined ? ">" : ""
                                        font.pixelSize: size / 2.0
                                    }
                                    onClicked: root.on_recordfx_clicked()

                                    ToolTip.delay: 1000
                                    ToolTip.timeout: 5000
                                    ToolTip.visible: hovered
                                    ToolTip.text: "Trigger FX re-record. This will play the full dry loop once with live FX, recording the result for wet playback."
                                }
                            }
                        }
                    }
                }

                SmallButtonWithCustomHover {
                    id : stop
                    width: buttongrid.button_width
                    height: buttongrid.button_height
                    IconWithText {
                        size: parent.width
                        anchors.centerIn: parent
                        name: 'stop'
                        color: Material.foreground
                        text_color: Material.foreground
                        text: root.delay_for_targeted != undefined ? ">" : ""
                        font.pixelSize: size / 2.0
                    }

                    onClicked: root.on_stop_clicked()

                    ToolTip.delay: 1000
                    ToolTip.timeout: 5000
                    ToolTip.visible: hovered
                    ToolTip.text: "Stop."

                    Connections {
                        target: statusrect
                        function onPropagateMousePosition(pt) { stop.onMousePosition(pt) }
                        function onPropagateMouseExited() { stop.onMouseExited() }
                    }
                }
            }

            Item {
                anchors.right: parent.right
                width: 18
                height: 18
                anchors.verticalCenter: parent.verticalCenter
                anchors.rightMargin: 5

                // Display the gain dial always
                AudioDial {
                    id: gain_dial
                    visible: root.descriptor_has_audio && !root.maybe_composite_loop
                    anchors.fill: parent
                    from: -30.0
                    to:   20.0
                    value: 0.0
                    property real initial_linear_value: root.initial_gain
                    onInitial_linear_valueChanged: set_from_linear(initial_linear_value)
                    Component.onCompleted: set_from_linear(initial_linear_value)

                    LinearDbConversion {
                        dB_threshold: parent.from
                        id: convert_gain
                    }

                    show_value_tooltip: true
                    value_tooltip_postfix: ' dB'

                    onMoved: {
                        convert_gain.dB = gain_dial.value
                        push_gain(convert_gain.linear)
                    }
                    function set_from_linear(val) {
                        convert_gain.linear = val
                        value = convert_gain.dB
                    }
                    function set_as_range_fraction(val) {
                        value = (val * (to - from)) + from
                    }

                    handle.width: 4
                    handle.height: 4
                    
                    background: Rectangle {
                        radius: width / 2.0
                        width: parent.width
                        color: '#222222'
                        border.width: 1
                        border.color: 'grey'
                    }

                    label: 'V'
                    tooltip: 'Loop gain. Double-click to reset.'

                    HoverHandler {
                        id: gain_dial_hover
                    }
                }

                Popup {
                    property bool visible_in: root.is_stereo && (gain_dial_hover.hovered || gain_dial.pressed || balance_dial_hover.hovered)
                    Timer {
                        id: visible_off_timer
                        interval: 50
                        repeat: false
                    }
                    onVisible_inChanged: { if (!visible_in) { visible_off_timer.restart(); } }
                    visible: visible_in || visible_off_timer.running
                    background: Rectangle { 
                        width: balance_dial.width
                        height: balance_dial.height
                        color: Material.background
                    }
                    leftInset: 0
                    rightInset: 0
                    topInset: 0
                    bottomInset: 0
                    padding: 0
                    margins: 0
                    x: parent.width + 4
                    y: 0

                    AudioDial {
                        visible: root.descriptor_has_audio && !root.maybe_composite_loop
                        id: balance_dial
                        from: -1.0
                        to:   1.0
                        value: root.is_stereo ? root.initial_stereo_balance : 0.0

                        width: gain_dial.width
                        height: gain_dial.height

                        onMoved: {
                            push_stereo_balance(value)
                        }

                        show_value_tooltip: true

                        handle.width: 4
                        handle.height: 4
                        
                        background: Rectangle {
                            radius: width / 2.0
                            width: parent.width
                            color: '#222222'
                            border.width: 1
                            border.color: 'grey'
                        }

                        HoverHandler {
                            id: balance_dial_hover
                        }

                        label: 'B'
                        tooltip: 'Loop stereo balance. Double-click to reset.'
                    }
                }
            }
        }
    }

    component LoopProgressRect : Item {
        id: loopprogressrect
        property var loop

        Rectangle {
            function getRightMargin() {
                var st = loopprogressrect.loop
                if(st && st.length && st.length > 0) {
                    return (1.0 - (st.display_position / st.length)) * parent.width
                }
                return parent.width
            }

            anchors {
                fill: parent
                rightMargin: getRightMargin()
            }
            color: {
                var default_color = '#444444'
                if (!loopprogressrect.loop) {
                    return default_color;
                }

                switch(loopprogressrect.loop.mode) {
                case ShoopConstants.LoopMode.Playing:
                    return '#004400';
                case ShoopConstants.LoopMode.PlayingDryThroughWet:
                    return '#333300';
                case ShoopConstants.LoopMode.Recording:
                    return '#660000';
                case ShoopConstants.LoopMode.RecordingDryIntoWet:
                    return '#663300';
                default:
                    return default_color;
                }
            }
        }

        Rectangle {
            visible: root.maybe_loop ?
                (root.maybe_loop.display_midi_events_triggered > 0 || root.maybe_loop.display_midi_notes_active > 0) :
                false
            anchors {
                right: parent.right
                top: parent.top
                bottom: parent.bottom
            }
            width: 8

            color: '#00FFFF'
        }
    }

    component LoopStateIcon : Item {
        id: lsicon
        property int mode
        property bool connected
        property bool show_timer_instead
        property bool is_regular_composite
        property bool is_script_composite
        property bool empty
        property bool muted
        property int size
        property string description: ModeHelpers.mode_name(mode)

        signal clicked(var mouse)
        signal doubleClicked(var mouse)

        width: size
        height: size

        IconWithText {
            id: main_icon
            anchors.fill: parent

            name: {
                if(!lsicon.connected) {
                    return 'cancel'
                }
                if(lsicon.show_timer_instead) {
                    return 'timer-sand'
                }
                if(lsicon.empty) {
                    return 'border-none-variant'
                }

                switch(lsicon.mode) {
                case ShoopConstants.LoopMode.Playing:
                case ShoopConstants.LoopMode.PlayingDryThroughWet:
                    return lsicon.muted ? 'volume-mute' : 'play'
                case ShoopConstants.LoopMode.Recording:
                case ShoopConstants.LoopMode.RecordingDryIntoWet:
                    return 'record-rec'
                case ShoopConstants.LoopMode.Stopped:
                    if(lsicon.is_regular_composite) {
                        return 'view-list'
                    }
                    if(lsicon.is_script_composite) {
                        return 'playlist-edit'
                    }
                    return 'stop'
                default:
                    return 'help-circle'
                }
            }

            color: {
                if(!lsicon.connected) {
                    return 'grey'
                }
                if(lsicon.show_timer_instead) {
                    return Material.foreground
                }
                switch(lsicon.mode) {
                case ShoopConstants.LoopMode.Playing:
                    if (lsicon.is_script_composite) {
                        return Material.foreground
                    }
                    return '#00AA00'
                case ShoopConstants.LoopMode.Recording:
                    return 'red'
                case ShoopConstants.LoopMode.RecordingDryIntoWet:
                case ShoopConstants.LoopMode.PlayingDryThroughWet:
                    return 'orange'
                default:
                    if(lsicon.is_regular_composite || lsicon.is_script_composite) {
                        return Material.background
                    }
                    return 'grey'
                }
            }

            text_color: Material.foreground
            text: {
                if(lsicon.show_timer_instead) {
                    return ''
                }
                switch(lsicon.mode) {
                case ShoopConstants.LoopMode.RecordingDryIntoWet:
                case ShoopConstants.LoopMode.PlayingDryThroughWet:
                    return 'FX'
                default:
                    return ''
                }
            }

            size: parent.size
        }

        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            propagateComposedEvents: true
            acceptedButtons: Qt.RightButton | Qt.MiddleButton | Qt.LeftButton
            onClicked: (mouse) => lsicon.clicked(mouse)
            onDoubleClicked: (mouse) => lsicon.doubleClicked(mouse)
            id: ma
        }

        ToolTip {
            delay: 1000
            visible: ma.containsMouse
            text: description
        }
    }

    component ContextMenu: Item {
        property alias opened: menu.opened
        visible: menu.visible

        ClickTrackDialog {
            id: clicktrackdialog
            loop: root
            parent: Overlay.overlay
            x: (parent.width-width) / 2
            y: (parent.height-height) / 2

            audio_enabled: root.descriptor_has_audio
            midi_enabled: root.descriptor_has_midi

            onAcceptedClickTrack: (kind, filename) => {
                if (kind == 'audio') {
                    loadoptionsdialog.filename = filename
                    close()
                    root.create_backend_loop()
                    loadoptionsdialog.update()
                    loadoptionsdialog.open()
                } else if (kind == 'midi') {
                    midiloadoptionsdialog.filename = filename
                    close()
                    root.create_backend_loop()
                    midiloadoptionsdialog.open()
                }
            }
        }

        Menu {
            id: menu
            title: 'Record'

            ShoopMenuItem {
                height: 50
                Row {
                    anchors.fill: parent
                    spacing: 5
                    anchors.leftMargin: 10
                    Text {
                        text: "Name:"
                        color: Material.foreground
                        anchors.verticalCenter: name_field.verticalCenter
                    }
                    ShoopTextField {
                        id: name_field
                        text: root.name
                        font.pixelSize: 12
                        onEditingFinished: {
                            root.name = text
                            focus = false
                            release_focus_notifier.notify()
                        }
                    }
                }
            }
            MenuSeparator {}
            ShoopMenuItem {
               text: "Details"
               onClicked: () => {
                  if(root.main_details_pane) { root.main_details_pane.add_user_item(root.name, root) }
                  registries.state_registry.set_details_open(true)
               }
            }
            ShoopMenuItem {
               text: "Click loop..."
               onClicked: () => clicktrackdialog.open()
            }
            ShoopMenuItem {
                text: "Create Composite"
                shown: !root.maybe_composite_loop && !root.is_sync
                onClicked: root.create_composite_loop()
            }
            ShoopMenuItem {
                text: "Save audio..."
                onClicked: { presavedialog.update(); presavedialog.open() }
                shown: root.descriptor_has_audio && root.maybe_backend_loop
            }
            ShoopMenuItem {
                text: "Load audio..."
                onClicked: loaddialog.open()
                shown: root.descriptor_has_audio
            }
            ShoopMenuItem {
                text: "Load MIDI..."
                shown: root.descriptor_has_midi
                onClicked: {
                    create_backend_loop()
                    var chans = root.midi_channels
                    if (chans.length == 0) { throw new Error("No MIDI channels to load"); }
                    if (chans.length > 1) { throw new Error("Cannot load into more than 1 MIDI channel"); }
                    midiloadoptionsdialog.channels = [chans[0]]
                    midiloaddialog.open()
                }
            }
            ShoopMenuItem {
                text: "Save MIDI..."
                shown: root.descriptor_has_midi && root.maybe_backend_loop
                onClicked: {
                    var chans = root.midi_channels
                    if (chans.length == 0) { throw new Error("No MIDI channels to save"); }
                    if (chans.length > 1) { throw new Error("Cannot save more than 1 MIDI channel"); }
                    midisavedialog.channel = chans[0]
                    midisavedialog.open()
                }
            }
            ShoopMenuItem {
                text: "Push Length To Sync"
                shown: !root.is_sync
                onClicked: {
                    if (sync_loop) { sync_loop.set_length(root.length) }
                }
            }
            ShoopMenuItem {
                text: "Clear"
                onClicked: () => {
                    root.clear(0)
                }
            }
            ShoopMenuItem {
                id: restore_fx_state_button

                property var cached_fx_state: {
                    if (!root.maybe_fx_chain) { return undefined; }
                    var channel_states = root.channels
                        .filter(c => c.recording_fx_chain_state_id != undefined)
                        .map(c => c.recording_fx_chain_state_id);
                    return channel_states.length > 0 ? 
                        registries.fx_chain_states_registry.maybe_get(channel_states[0], undefined)
                        : undefined
                }

                text: "Restore FX State"
                shown: cached_fx_state ? true : false
                onClicked: root.maybe_fx_chain.restore_state(cached_fx_state.internal_state)
            }
        }

        Dialog {
            id: presavedialog
            standardButtons: Dialog.Save | Dialog.Cancel
            parent: Overlay.overlay
            x: (parent.width - width) / 2
            y: (parent.height - height) / 2
            modal: true
            readonly property int n_channels: channels.length
            property var channels: []

            width: 300
            height: 200

            function update() {
                var chans = root.audio_channels
                channels = chans.filter(select_channels.currentValue)
                footer.standardButton(Dialog.Save).enabled = n_channels > 0;
                savedialog.channels = channels
            }

            onAccepted: { close(); savedialog.open() }

            Column {
                Row {
                    Label {
                        text: "Include channels:"
                        anchors.verticalCenter: select_channels.verticalCenter
                    }
                    ShoopComboBox {
                        id: select_channels
                        textRole: "text"
                        valueRole: "value"
                        model: [
                            { value: (chan) => true, text: "All" },
                            { value: (chan) => chan.mode == ShoopConstants.ChannelMode.Direct, text: "Regular" },
                            { value: (chan) => chan.mode == ShoopConstants.ChannelMode.Dry, text: "Dry" },
                            { value: (chan) => chan.mode == ShoopConstants.ChannelMode.Wet, text: "Wet" }
                        ]
                        Component.onCompleted: presavedialog.update()
                        onActivated: presavedialog.update()
                    }
                }
                Label {
                    text: {
                        switch(presavedialog.n_channels) {
                            case 0:
                                return 'No channels match the selected filter.'
                            case 1:
                                return 'Format: mono audio'
                            case 2:
                                return 'Format: stereo audio'
                            default:
                                return 'Format: ' + presavedialog.n_channels.toString() + '-channel audio'
                        }
                    }
                }
            }
        }

        ShoopFileDialog {
            id: savedialog
            fileMode: FileDialog.SaveFile
            acceptLabel: 'Save'
            nameFilters: Object.entries(file_io.get_soundfile_formats()).map((e) => {
                var extension = e[0]
                var description = e[1].replace('(', '- ').replace(')', '')
                return description + ' (*.' + extension + ')';
            })
            property var channels: []
            onAccepted: {
                if (!root.maybe_backend_loop) { 
                    root.logger.error(() => ("Cannot save: loop not loaded"))
                    return;
                }
                close()
                registries.state_registry.save_action_started()
                try {
                    var filename = UrlToFilename.qml_url_to_filename(file.toString());
                    var samplerate = root.maybe_backend_loop.backend.get_sample_rate()
                    var task = file_io.save_channels_to_soundfile_async(filename, samplerate, channels)
                    task.when_finished(() => registries.state_registry.save_action_finished())
                } catch (e) {
                    registries.state_registry.save_action_finished()
                    throw e;
                }
            }
            
        }

        ShoopFileDialog {
            id: midisavedialog
            fileMode: FileDialog.SaveFile
            acceptLabel: 'Save'
            nameFilters: ["MIDI files (*.mid)", "Sample-accurate Shoop MIDI (*.smf)"]
            property var channel: null
            onAccepted: {
                if (!root.maybe_backend_loop) { 
                    root.logger.error(() => ("Cannot save: loop not loaded"))
                    return;
                }
                close()
                var filename = UrlToFilename.qml_url_to_filename(file.toString());
                var samplerate = root.maybe_backend_loop.backend.get_sample_rate()
                file_io.save_channel_to_midi_async(filename, samplerate, channel)
            }
            
        }

        ShoopFileDialog {
            id: loaddialog
            fileMode: FileDialog.OpenFile
            acceptLabel: 'Load'
            nameFilters: [
                'Supported sound files ('
                + Object.entries(file_io.get_soundfile_formats()).map((e) => '*.' + e[0].toLowerCase()).join(' ')
                + ')'
            ]
            onAccepted: {
                loadoptionsdialog.filename = UrlToFilename.qml_url_to_filename(file.toString());
                close()
                root.create_backend_loop()
                loadoptionsdialog.update()
                loadoptionsdialog.open()
            }
            
        }

        Dialog {
            id: loadoptionsdialog
            standardButtons: Dialog.Open | Dialog.Cancel
            parent: Overlay.overlay
            x: (parent.width - width) / 2
            y: (parent.height - height) / 2
            
            modal: true
            property string filename: ''
            property var channels_to_load : []
            property var direct_audio_channels : []
            property var dry_audio_channels : []
            property var wet_audio_channels : []
            readonly property int n_channels : channels_to_load.length
            property int n_file_channels : 0
            property int file_sample_rate : 0
            property int backend_sample_rate : root.maybe_backend_loop && root.maybe_backend_loop.backend ? root.maybe_backend_loop.backend.get_sample_rate() : 0
            property bool will_resample : file_sample_rate != backend_sample_rate

            width: 300
            height: 400

            onFilenameChanged: {
                var props = file_io.get_soundfile_info(filename)
                n_file_channels = props['channels']
                file_sample_rate = props['samplerate']
            }
            onStandardButtonsChanged: update()

            function update() {
                var chans = root.audio_channels
                direct_audio_channels = chans.filter(c => c.mode == ShoopConstants.ChannelMode.Direct)
                dry_audio_channels = chans.filter(c => c.mode == ShoopConstants.ChannelMode.Dry)
                wet_audio_channels = chans.filter(c => c.mode == ShoopConstants.ChannelMode.Wet)
                var to_load = []
                if (direct_load_checkbox.checked) { to_load = to_load.concat(direct_audio_channels) }
                if (dry_load_checkbox.checked) { to_load = to_load.concat(dry_audio_channels) }
                if (wet_load_checkbox.checked) { to_load = to_load.concat(wet_audio_channels) }
                channels_to_load = to_load
                if(footer.standardButton(Dialog.Open)) {
                    footer.standardButton(Dialog.Open).enabled = channels_to_load.length > 0;
                }
            }

            // TODO: there has to be a better way
            Timer {                                                                                                                                                                                 
                interval: 100
                running: loadoptionsdialog.opened
                onTriggered: loadoptionsdialog.update()
            }

            onAccepted: {
                if (!root.maybe_backend_loop) { 
                    root.logger.error(() => ("Cannot load: loop not loaded"))
                    return;
                }
                registries.state_registry.load_action_started()
                try {
                    close()
                    var samplerate = root.maybe_backend_loop.backend.get_sample_rate()
                    // Distribute file channels round-robin over loop channels.
                    // TODO: provide other options
                    var mapping = Array.from(Array(n_file_channels).keys()).map(v => [])
                    var fidx=0;
                    for(var cidx = 0; cidx < n_channels; cidx++) {
                        mapping[fidx].push(channels_to_load[cidx])
                        fidx = (fidx + 1) % n_file_channels
                    }
                    var task = file_io.load_soundfile_to_channels_async(filename, samplerate, null,
                        mapping, 0, 0, root.maybe_backend_loop)
                    task.when_finished( () => registries.state_registry.load_action_finished() )
                } catch(e) {
                    registries.state_registry.load_action_finished()
                    throw e
                }
            }

            Column {
                Grid {
                    columns: 2
                    verticalItemAlignment: Grid.AlignVCenter
                    columnSpacing: 10

                    Label {
                        text: "To direct channel(s):"
                        visible: loadoptionsdialog.direct_audio_channels.length > 0
                    }
                    CheckBox {
                        id: direct_load_checkbox
                        visible: loadoptionsdialog.direct_audio_channels.length > 0
                        checked: true
                        onCheckedChanged: loadoptionsdialog.update()
                    }

                    Label {
                        text: "To dry channel(s):"
                        visible: loadoptionsdialog.dry_audio_channels.length > 0
                    }
                    CheckBox {
                        id: dry_load_checkbox
                        visible: loadoptionsdialog.dry_audio_channels.length > 0
                        checked: true
                        onCheckedChanged: loadoptionsdialog.update()
                    }

                    Label {
                        text: "To wet channel(s):"
                        visible: loadoptionsdialog.wet_audio_channels.length > 0
                    }
                    CheckBox {
                        id: wet_load_checkbox
                        visible: loadoptionsdialog.wet_audio_channels.length > 0
                        checked: true
                        onCheckedChanged: loadoptionsdialog.update()
                    }

                    Label {
                        text: "Update loop length:"
                    }
                    CheckBox {
                        id: update_audio_length_checkbox
                        checked: true
                    }
                }

                Label {
                    visible: loadoptionsdialog.will_resample
                    wrapMode: Text.WordWrap
                    width: loadoptionsdialog.width - 50
                    text: "Warning: the file to load will be resampled. This may take a long time."
                }
            }
        }

        ShoopFileDialog {
            id: midiloaddialog
            fileMode: FileDialog.OpenFile
            acceptLabel: 'Load'
            nameFilters: ["Midi files (*.mid)"]
            onAccepted: {
                close()
                midiloadoptionsdialog.filename = UrlToFilename.qml_url_to_filename(file.toString());
                midiloadoptionsdialog.open()
            }
        }

        Dialog {
            id: midiloadoptionsdialog
            standardButtons: Dialog.Yes | Dialog.No
            Label { text: "Update loop length to loaded data length?" }
            property string filename
            property var channels : root.midi_channels
            function doLoad(update_loop_length) {
                root.create_backend_loop()
                var samplerate = root.maybe_backend_loop.backend.get_sample_rate()
                file_io.load_midi_to_channels_async(filename, samplerate, channels,
                    0, 0, root.maybe_backend_loop)
            }

            onAccepted: doLoad(true)
            onRejected: doLoad(false)
        }

        function popup () {
            menu.popup()
        }
    }
}
