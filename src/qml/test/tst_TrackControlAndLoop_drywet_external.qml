import QtQuick 6.6

import ShoopDaLoop.Rust
import '../js/generate_session.js' as GenerateSession
import './testfilename.js' as TestFilename
import '..'

ShoopTestFile {
    TestSession {
        id: session

        anchors.fill: parent
        initial_descriptor: {
            let monitored_track = GenerateSession.generate_default_track(
                "monitored",
                1,
                "monitored",
                false,
                "monitored",
                1,
                1,
                0,
                true,
                false,
                true,
                undefined
            )
            let muted_track = GenerateSession.generate_default_track(
                "muted",
                1,
                "muted",
                false,
                "muted",
                1,
                1,
                0,
                true,
                false,
                true,
                undefined
            )
            monitored_track.ports
                .filter((port) => port.id.match(/(?:audio_dry_in|dry_midi_in)/))
                .forEach((port) => port.passthrough_muted = false)
            muted_track.ports
                .filter((port) => port.id.match(/(?:audio_dry_in|dry_midi_in)/))
                .forEach((port) => port.passthrough_muted = false)
            return GenerateSession.generate_default_session(
                global_args.version_string,
                null,
                true,
                1,
                1,
                [monitored_track, muted_track]
            )
        }

        ShoopSessionTestCase {
            id: testcase
            name: 'TrackControlAndLoop_drywet_external'
            filename: TestFilename.test_filename()
            session: session

            property var monitored_track: session.main_tracks[0]
            property var muted_track: session.main_tracks[1]
            property var monitored_loop: monitored_track.loops[0]
            property var muted_loop: muted_track.loops[0]
            property var sync_loop: session.sync_track.loops[0]

            RegistryLookup {
                id: lookup_monitored_midi_input
                registry: AppRegistries.objects_registry
                key: "monitored_dry_midi_in"
            }
            property alias monitored_midi_input: lookup_monitored_midi_input.object

            RegistryLookup {
                id: lookup_monitored_midi_send
                registry: AppRegistries.objects_registry
                key: "monitored_dry_midi_send"
            }
            property alias monitored_midi_send: lookup_monitored_midi_send.object

            RegistryLookup {
                id: lookup_monitored_audio_input
                registry: AppRegistries.objects_registry
                key: "monitored_audio_dry_in_1"
            }
            property alias monitored_audio_input: lookup_monitored_audio_input.object

            RegistryLookup {
                id: lookup_monitored_audio_send
                registry: AppRegistries.objects_registry
                key: "monitored_audio_dry_send_1"
            }
            property alias monitored_audio_send: lookup_monitored_audio_send.object

            RegistryLookup {
                id: lookup_muted_midi_input
                registry: AppRegistries.objects_registry
                key: "muted_dry_midi_in"
            }
            property alias muted_midi_input: lookup_muted_midi_input.object

            RegistryLookup {
                id: lookup_muted_midi_send
                registry: AppRegistries.objects_registry
                key: "muted_dry_midi_send"
            }
            property alias muted_midi_send: lookup_muted_midi_send.object

            RegistryLookup {
                id: lookup_monitored_wet_return
                registry: AppRegistries.objects_registry
                key: "monitored_audio_wet_return_1"
            }
            property alias monitored_wet_return: lookup_monitored_wet_return.object

            RegistryLookup {
                id: lookup_monitored_wet_output
                registry: AppRegistries.objects_registry
                key: "monitored_audio_wet_out_1"
            }
            property alias monitored_wet_output: lookup_monitored_wet_output.object

            testcase_init_fn: () => {
                session.backend.dummy_enter_controlled_mode()
                testcase.wait_controlled_mode(session.backend)
                verify_true(monitored_track)
                verify_true(muted_track)
                verify_true(monitored_loop)
                verify_true(muted_loop)
                verify_true(sync_loop)
                verify_true(monitored_midi_input)
                verify_true(monitored_midi_send)
                verify_true(monitored_audio_input)
                verify_true(monitored_audio_send)
                verify_true(muted_midi_input)
                verify_true(muted_midi_send)
                verify_true(monitored_wet_return)
                verify_true(monitored_wet_output)
                monitored_loop.create_backend_loop()
                muted_loop.create_backend_loop()
                sync_loop.create_backend_loop()
                reset()
            }

            function reset_track(track) {
                track.control_widget.monitor = false
                track.control_widget.mute = false
            }

            function reset_loop(loop) {
                loop.transition(
                    ShoopRustConstants.LoopMode.Stopped,
                    ShoopRustConstants.DontWaitForSync,
                    ShoopRustConstants.DontAlignToSyncImmediately
                )
                testcase.wait_updated(session.backend)
                loop.clear(0)
                loop.midi_channels.forEach((channel) => channel.reset_state_tracking())
            }

            function clear_midi_queues() {
                monitored_midi_input.dummy_clear_queues()
                monitored_midi_send.dummy_clear_queues()
                muted_midi_input.dummy_clear_queues()
                muted_midi_send.dummy_clear_queues()
            }

            function reset() {
                reset_track(session.sync_track)
                reset_track(monitored_track)
                reset_track(muted_track)
                reset_loop(monitored_loop)
                reset_loop(muted_loop)
                reset_loop(sync_loop)
                clear_midi_queues()
                session.backend.wait_process()
                testcase.wait_updated(session.backend)
            }

            function monitored_dry_channel() {
                return monitored_loop.get_audio_channels()
                    .find((channel) => channel.obj_id.match(/.*_dry_.*/))
            }

            function monitored_wet_channel() {
                return monitored_loop.get_audio_channels()
                    .find((channel) => channel.obj_id.match(/.*_wet_.*/))
            }

            function monitored_midi_channel() {
                return monitored_loop.midi_channels[0]
            }

            function add_audio(a, b) {
                return a.map((value, index) => value + b[index])
            }

            function is_cleanup_for_note(message, note) {
                let data = message.data
                let status = data[0] & 0xF0
                return (status === 0x80 && data[1] === note) ||
                    (status === 0x90 && data[1] === note && data[2] === 0) ||
                    (status === 0xB0 && (data[1] === 120 || data[1] === 123))
            }

            function verify_note_cleanup(messages, note) {
                verify_true(
                    messages.some((message) => is_cleanup_for_note(message, note)),
                    `Expected cleanup for MIDI note ${note}, observed ${JSON.stringify(messages)}`
                )
            }

            function queue_note_on_and_collect(note) {
                monitored_midi_input.dummy_queue_midi_msgs([
                    { 'time': 0, 'data': [0x90, note, 100] }
                ])
                monitored_midi_send.dummy_request_data(1)
                session.backend.dummy_request_controlled_frames(1)
                session.backend.dummy_run_requested_frames()
                return monitored_midi_send.dummy_dequeue_midi_msgs()
            }

            function collect_midi_send(n_frames) {
                monitored_midi_send.dummy_request_data(n_frames)
                session.backend.dummy_request_controlled_frames(n_frames)
                session.backend.dummy_run_requested_frames()
                testcase.wait_updated(session.backend)
                return monitored_midi_send.dummy_dequeue_midi_msgs()
            }

            function run_external_mode_matrix(mode, requested_monitor) {
                check_backend()
                reset()

                let live_audio = [1, 2, 3, 4]
                let dry_audio = [5, 6, 7, 8]
                let wet_return = [10, 20, 30, 40]
                let wet_audio = [50, 60, 70, 80]
                let silence = [0, 0, 0, 0]
                let live_midi = [
                    { 'time': 0, 'data': [0x90, 80, 100] },
                    { 'time': 3, 'data': [0x80, 80, 0] }
                ]
                let dry_midi = [
                    { 'time': 1, 'data': [0x90, 81, 100] },
                    { 'time': 2, 'data': [0x80, 81, 0] }
                ]
                let has_existing_content = [
                    ShoopRustConstants.LoopMode.Playing,
                    ShoopRustConstants.LoopMode.Replacing,
                    ShoopRustConstants.LoopMode.PlayingDryThroughWet,
                    ShoopRustConstants.LoopMode.RecordingDryIntoWet
                ].includes(mode)
                if (has_existing_content) {
                    monitored_dry_channel().load_data(dry_audio)
                    monitored_wet_channel().load_data(wet_audio)
                    monitored_midi_channel().load_midi_data(dry_midi)
                    monitored_loop.queue_set_length(4)
                }

                monitored_track.control_widget.monitor = requested_monitor
                monitored_loop.transition(
                    mode,
                    ShoopRustConstants.DontWaitForSync,
                    ShoopRustConstants.DontAlignToSyncImmediately
                )
                testcase.wait_updated(session.backend)

                let rerecording = mode === ShoopRustConstants.LoopMode.RecordingDryIntoWet
                verify_eq(monitored_track.control_widget.monitor, rerecording ? false : requested_monitor)

                monitored_audio_input.dummy_queue_audio_data(live_audio)
                monitored_midi_input.dummy_queue_midi_msgs(live_midi)
                monitored_wet_return.dummy_queue_audio_data(wet_return)
                monitored_audio_send.dummy_request_data(4)
                monitored_midi_send.dummy_request_data(4)
                monitored_wet_output.dummy_request_data(4)
                session.backend.dummy_request_controlled_frames(4)
                session.backend.dummy_run_requested_frames()
                testcase.wait_updated(session.backend)

                let sent_audio = monitored_audio_send.dummy_dequeue_audio_data(4)
                let sent_midi = monitored_midi_send.dummy_dequeue_midi_msgs()
                let output = monitored_wet_output.dummy_dequeue_audio_data(4)

                let recording = mode === ShoopRustConstants.LoopMode.Recording ||
                    mode === ShoopRustConstants.LoopMode.Replacing
                let playing_wet = mode === ShoopRustConstants.LoopMode.Playing
                let playing_dry = mode === ShoopRustConstants.LoopMode.PlayingDryThroughWet || rerecording
                let live_input_enabled = recording || (requested_monitor && !rerecording)
                let return_enabled = requested_monitor || playing_dry

                let expected_send_audio = playing_dry ? dry_audio : silence
                if (live_input_enabled) {
                    expected_send_audio = add_audio(expected_send_audio, live_audio)
                }
                let expected_send_midi = []
                if (live_input_enabled) {
                    expected_send_midi = expected_send_midi.concat(live_midi)
                }
                if (playing_dry) {
                    expected_send_midi = expected_send_midi.concat(dry_midi)
                    expected_send_midi.sort((a, b) => a.time - b.time)
                }
                let expected_output = playing_wet ? wet_audio : silence
                if (return_enabled) {
                    expected_output = add_audio(expected_output, wet_return)
                }

                verify_eq(sent_audio, expected_send_audio)
                verify_eq(sent_midi, expected_send_midi, null, true)
                verify_eq(output, expected_output)

                if (recording || rerecording) {
                    monitored_loop.transition(
                        ShoopRustConstants.LoopMode.Stopped,
                        ShoopRustConstants.DontWaitForSync,
                        ShoopRustConstants.DontAlignToSyncImmediately
                    )
                    testcase.wait_updated(session.backend)
                    session.backend.dummy_request_controlled_frames(1)
                    session.backend.dummy_run_requested_frames()
                    testcase.wait_updated(session.backend)
                }
                if (recording) {
                    verify_eq(monitored_dry_channel().get_data(), live_audio)
                    verify_eq(monitored_wet_channel().get_data(), wet_return)
                    verify_eq(monitored_midi_channel().get_recorded_midi_msgs(), live_midi, null, true)
                } else if (rerecording) {
                    verify_eq(monitored_dry_channel().get_data(), dry_audio)
                    verify_eq(monitored_wet_channel().get_data(), wet_return)
                    verify_eq(monitored_midi_channel().get_recorded_midi_msgs(), dry_midi, null, true)
                }
            }

            test_fns: ({
                // Purpose: A stopped, unmonitored external track must gate every live dry/wet path.
                // Use case: An idle external-effects track must remain silent while another track is used.
                'test_external_matrix_stopped_monitor_off': () => {
                    run_external_mode_matrix(ShoopRustConstants.LoopMode.Stopped, false)
                },

                // Purpose: A stopped, monitored external track must forward live dry input and wet return.
                // Use case: A performer plays through an external processor before recording a loop.
                'test_external_matrix_stopped_monitor_on': () => {
                    run_external_mode_matrix(ShoopRustConstants.LoopMode.Stopped, true)
                },

                // Purpose: Unmonitored recording must send and capture live dry/wet data without live output.
                // Use case: A performer records an external processor silently while monitoring elsewhere.
                'test_external_matrix_recording_monitor_off': () => {
                    run_external_mode_matrix(ShoopRustConstants.LoopMode.Recording, false)
                },

                // Purpose: Monitored recording must send, capture, and audition the external wet return.
                // Use case: A performer hears an external processor while recording its dry and wet signals.
                'test_external_matrix_recording_monitor_on': () => {
                    run_external_mode_matrix(ShoopRustConstants.LoopMode.Recording, true)
                },

                // Purpose: Unmonitored replacement must replace dry/wet data without live wet output.
                // Use case: A performer overwrites an existing loop while monitoring through another path.
                // Failure: Expected dry MIDI [{time:0,note-on 80},{time:3,note-off 80}]; observed
                // [{time:1,note-on 81},{time:2,note-off 81}]. Loaded MIDI replacement may not commit.
                'test_external_matrix_replacing_monitor_off': () => {
                    run_external_mode_matrix(ShoopRustConstants.LoopMode.Replacing, false)
                },

                // Purpose: Monitored replacement must replace and audition the external wet return.
                // Use case: A performer hears the processor while overwriting an existing loop.
                // Failure: Expected dry MIDI [{time:0,note-on 80},{time:3,note-off 80}]; observed
                // [{time:1,note-on 81},{time:2,note-off 81}]. Loaded MIDI replacement may not commit.
                'test_external_matrix_replacing_monitor_on': () => {
                    run_external_mode_matrix(ShoopRustConstants.LoopMode.Replacing, true)
                },

                // Purpose: Normal unmonitored playback must emit only stored wet audio.
                // Use case: A recorded wet loop plays without retriggering or mixing the external processor.
                'test_external_matrix_playing_monitor_off': () => {
                    run_external_mode_matrix(ShoopRustConstants.LoopMode.Playing, false)
                },

                // Purpose: Normal monitored playback must combine stored wet audio with the live wet return.
                // Use case: A performer plays live through the processor over an existing wet loop.
                'test_external_matrix_playing_monitor_on': () => {
                    run_external_mode_matrix(ShoopRustConstants.LoopMode.Playing, true)
                },

                // Purpose: Unmonitored dry playback must send only recorded dry audio/MIDI and hear its return.
                // Use case: A loop is re-auditioned through external effects without live-input contamination.
                'test_external_matrix_playdry_monitor_off': () => {
                    run_external_mode_matrix(ShoopRustConstants.LoopMode.PlayingDryThroughWet, false)
                },

                // Purpose: Monitored dry playback must combine recorded dry material with live processor input.
                // Use case: A performer plays alongside a dry loop through the same external processor.
                'test_external_matrix_playdry_monitor_on': () => {
                    run_external_mode_matrix(ShoopRustConstants.LoopMode.PlayingDryThroughWet, true)
                },

                // Purpose: Dry re-recording must send stored dry data and replace wet data with the return.
                // Use case: An existing dry loop is committed through newly configured external effects.
                'test_external_matrix_rerecord_monitor_off': () => {
                    run_external_mode_matrix(ShoopRustConstants.LoopMode.RecordingDryIntoWet, false)
                },

                // Purpose: Dry re-recording must force requested monitoring off and exclude live input.
                // Use case: Reprocessing starts while the track was monitored, avoiding mixed live material.
                'test_external_matrix_rerecord_monitor_on_forced_off': () => {
                    run_external_mode_matrix(ShoopRustConstants.LoopMode.RecordingDryIntoWet, true)
                },

                // Purpose: Muting monitoring must clean up notes already sent to an external synth.
                // Use case: A performer releases a held key after switching input monitoring off.
                // Failure: Expected a note-off, zero-velocity note-on, CC120, or CC123 for note 72;
                // observed []. The passthrough mute likely drops later events without flushing note state.
                'test_midi_cleanup_when_monitoring_is_disabled': () => {
                    check_backend()
                    reset()
                    monitored_track.control_widget.monitor = true
                    testcase.wait_updated(session.backend)

                    let note = 72
                    let started = queue_note_on_and_collect(note)
                    verify_eq(started, [
                        { 'time': 0, 'data': [0x90, note, 100] }
                    ], null, true)

                    monitored_track.control_widget.monitor = false
                    testcase.wait_updated(session.backend)
                    monitored_midi_input.dummy_queue_midi_msgs([
                        { 'time': 0, 'data': [0x80, note, 0] }
                    ])
                    let cleanup = collect_midi_send(1)
                    verify_note_cleanup(cleanup, note)
                },

                // Purpose: Immediate recording-to-playback must clean a live note before dry MIDI is gated.
                // Use case: A performer holds a key across the end of an unsynchronized recording.
                // Failure: Expected a note-off, zero-velocity note-on, CC120, or CC123 for note 73;
                // observed []. Immediate mode changes likely gate passthrough without flushing note state.
                'test_midi_cleanup_for_held_note_on_immediate_record_to_play': () => {
                    check_backend()
                    reset()
                    monitored_loop.transition(
                        ShoopRustConstants.LoopMode.Recording,
                        ShoopRustConstants.DontWaitForSync,
                        ShoopRustConstants.DontAlignToSyncImmediately
                    )
                    testcase.wait_updated(session.backend)

                    let note = 73
                    let started = queue_note_on_and_collect(note)
                    verify_eq(started, [
                        { 'time': 0, 'data': [0x90, note, 100] }
                    ], null, true)

                    monitored_loop.transition(
                        ShoopRustConstants.LoopMode.Playing,
                        ShoopRustConstants.DontWaitForSync,
                        ShoopRustConstants.DontAlignToSyncImmediately
                    )
                    testcase.wait_updated(session.backend)
                    monitored_midi_input.dummy_queue_midi_msgs([
                        { 'time': 0, 'data': [0x80, note, 0] }
                    ])
                    let cleanup = collect_midi_send(1)
                    verify_note_cleanup(cleanup, note)
                },

                // Purpose: A synchronized recording boundary must not leave an external note active.
                // Use case: A synchronized loop finishes recording while the performer still holds a key.
                'test_midi_cleanup_for_held_note_on_synchronized_record_to_play': () => {
                    check_backend()
                    reset()
                    sync_loop.queue_set_length(4)
                    sync_loop.transition(
                        ShoopRustConstants.LoopMode.Playing,
                        ShoopRustConstants.DontWaitForSync,
                        ShoopRustConstants.DontAlignToSyncImmediately
                    )
                    monitored_loop.transition(
                        ShoopRustConstants.LoopMode.Recording,
                        ShoopRustConstants.DontWaitForSync,
                        ShoopRustConstants.DontAlignToSyncImmediately
                    )
                    testcase.wait_updated(session.backend)

                    let note = 74
                    let started = queue_note_on_and_collect(note)
                    verify_eq(started, [
                        { 'time': 0, 'data': [0x90, note, 100] }
                    ], null, true)

                    monitored_loop.transition(
                        ShoopRustConstants.LoopMode.Playing,
                        0,
                        ShoopRustConstants.DontAlignToSyncImmediately
                    )
                    testcase.wait_updated(session.backend)
                    monitored_midi_input.dummy_queue_midi_msgs([
                        { 'time': 3, 'data': [0x80, note, 0] }
                    ])
                    let cleanup = collect_midi_send(4)
                    verify_eq(monitored_loop.mode, ShoopRustConstants.LoopMode.Playing)
                    verify_note_cleanup(cleanup, note)
                },

                // Purpose: Forced monitor-off during dry re-recording must clean live external notes.
                // Use case: A performer starts reprocessing a loop while a monitored key is held.
                // Failure: Expected a note-off, zero-velocity note-on, CC120, or CC123 for note 75;
                // observed []. Forced monitoring-off likely mutes passthrough without flushing note state.
                'test_midi_cleanup_when_rerecord_forces_monitoring_off': () => {
                    check_backend()
                    reset()
                    monitored_track.control_widget.monitor = true
                    monitored_loop.queue_set_length(4)
                    testcase.wait_updated(session.backend)

                    let note = 75
                    let started = queue_note_on_and_collect(note)
                    verify_eq(started, [
                        { 'time': 0, 'data': [0x90, note, 100] }
                    ], null, true)

                    monitored_loop.transition(
                        ShoopRustConstants.LoopMode.RecordingDryIntoWet,
                        ShoopRustConstants.DontWaitForSync,
                        ShoopRustConstants.DontAlignToSyncImmediately
                    )
                    testcase.wait_updated(session.backend)
                    verify_eq(monitored_track.control_widget.monitor, false)
                    let cleanup = collect_midi_send(1)
                    verify_note_cleanup(cleanup, note)
                },

                'test_input_muted_external_midi_is_not_sent': () => {
                    check_backend()
                    reset()
                    monitored_track.control_widget.monitor = false
                    testcase.wait_updated(session.backend)

                    let input = [
                        { 'time': 0, 'data': [0x90, 60, 100] },
                        { 'time': 3, 'data': [0x80, 60, 0] }
                    ]
                    monitored_midi_input.dummy_queue_midi_msgs(input)
                    monitored_midi_send.dummy_request_data(4)
                    testcase.wait_updated(session.backend)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_run_requested_frames()

                    let sent = monitored_midi_send.dummy_dequeue_midi_msgs()
                    verify_eq(sent, [], null, true)
                },

                'test_input_monitored_external_midi_is_sent': () => {
                    check_backend()
                    reset()
                    monitored_track.control_widget.monitor = true
                    testcase.wait_updated(session.backend)

                    let input = [
                        { 'time': 0, 'data': [0x90, 60, 100] },
                        { 'time': 3, 'data': [0x80, 60, 0] }
                    ]
                    monitored_midi_input.dummy_queue_midi_msgs(input)
                    monitored_midi_send.dummy_request_data(4)
                    testcase.wait_updated(session.backend)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_run_requested_frames()

                    let sent = monitored_midi_send.dummy_dequeue_midi_msgs()
                    verify_eq(sent, input, null, true)
                },

                'test_normal_playback_excludes_external_wet_return': () => {
                    check_backend()
                    reset()
                    monitored_track.control_widget.monitor = false
                    monitored_wet_channel().load_data([5, 6, 7, 8])
                    monitored_loop.queue_set_length(4)
                    monitored_loop.transition(
                        ShoopRustConstants.LoopMode.Playing,
                        ShoopRustConstants.DontWaitForSync,
                        ShoopRustConstants.DontAlignToSyncImmediately
                    )
                    testcase.wait_updated(session.backend)

                    monitored_wet_return.dummy_queue_audio_data([1, 2, 3, 4])
                    monitored_wet_output.dummy_request_data(4)
                    testcase.wait_updated(session.backend)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_run_requested_frames()

                    let output = monitored_wet_output.dummy_dequeue_audio_data(4)
                    verify_eq(output, [5, 6, 7, 8])
                },

                'test_immediately_recorded_dry_midi_is_not_sent_in_normal_playback': () => {
                    check_backend()
                    reset()
                    monitored_track.control_widget.monitor = false
                    monitored_loop.transition(
                        ShoopRustConstants.LoopMode.Recording,
                        ShoopRustConstants.DontWaitForSync,
                        ShoopRustConstants.DontAlignToSyncImmediately
                    )
                    testcase.wait_updated(session.backend)

                    let input = [
                        { 'time': 0, 'data': [0x90, 60, 100] },
                        { 'time': 3, 'data': [0x80, 60, 0] }
                    ]
                    monitored_midi_input.dummy_queue_midi_msgs(input)
                    monitored_midi_send.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_run_requested_frames()
                    monitored_midi_send.dummy_dequeue_midi_msgs()
                    monitored_midi_input.dummy_clear_queues()

                    monitored_loop.transition(
                        ShoopRustConstants.LoopMode.Playing,
                        ShoopRustConstants.DontWaitForSync,
                        ShoopRustConstants.DontAlignToSyncImmediately
                    )
                    testcase.wait_updated(session.backend)
                    monitored_midi_send.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_run_requested_frames()

                    let sent = monitored_midi_send.dummy_dequeue_midi_msgs()
                    verify_eq(sent, [], null, true)
                },

                'test_prerecorded_dry_midi_is_not_sent_in_normal_playback': () => {
                    check_backend()
                    reset()
                    monitored_track.control_widget.monitor = false

                    sync_loop.queue_set_length(4)
                    sync_loop.transition(
                        ShoopRustConstants.LoopMode.Playing,
                        ShoopRustConstants.DontWaitForSync,
                        ShoopRustConstants.DontAlignToSyncImmediately
                    )
                    testcase.wait_updated(session.backend)
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.dummy_run_requested_frames()

                    monitored_loop.transition(
                        ShoopRustConstants.LoopMode.Recording,
                        0,
                        ShoopRustConstants.DontAlignToSyncImmediately
                    )
                    testcase.wait_updated(session.backend)
                    let input = [
                        { 'time': 3, 'data': [0x90, 60, 100] },
                        { 'time': 4, 'data': [0x80, 60, 0] }
                    ]
                    monitored_midi_input.dummy_queue_midi_msgs(input)
                    monitored_midi_send.dummy_request_data(6)
                    session.backend.dummy_request_controlled_frames(6)
                    session.backend.dummy_run_requested_frames()
                    monitored_midi_send.dummy_dequeue_midi_msgs()
                    monitored_midi_input.dummy_clear_queues()

                    monitored_loop.transition(
                        ShoopRustConstants.LoopMode.Playing,
                        ShoopRustConstants.DontWaitForSync,
                        ShoopRustConstants.DontAlignToSyncImmediately
                    )
                    testcase.wait_updated(session.backend)
                    monitored_midi_send.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_run_requested_frames()

                    let sent = monitored_midi_send.dummy_dequeue_midi_msgs()
                    verify_eq(sent, [], null, true)
                },

                'test_shared_external_midi_only_reaches_input_monitored_track': () => {
                    check_backend()
                    reset()
                    monitored_track.control_widget.monitor = true
                    muted_track.control_widget.monitor = false
                    testcase.wait_updated(session.backend)

                    let input = [
                        { 'time': 0, 'data': [0x90, 60, 100] },
                        { 'time': 3, 'data': [0x80, 60, 0] }
                    ]
                    monitored_midi_input.dummy_queue_midi_msgs(input)
                    muted_midi_input.dummy_queue_midi_msgs(input)
                    monitored_midi_send.dummy_request_data(4)
                    muted_midi_send.dummy_request_data(4)
                    testcase.wait_updated(session.backend)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_run_requested_frames()

                    let monitored_sent = monitored_midi_send.dummy_dequeue_midi_msgs()
                    let muted_sent = muted_midi_send.dummy_dequeue_midi_msgs()
                    verify_eq(monitored_sent, input, null, true)
                    verify_eq(muted_sent, [], null, true)
                }
            })
        }
    }
}
