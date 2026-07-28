# C++ backend Catch unit-test porting map

This file records the direct Rust `shoop_engine` equivalents for the C++ Catch unit tests in
`../shoopdaloop/src/backend/test/unit`.  The Rust tests may be direct ports or, where the Rust
engine decomposes the old JACK/dummy abstractions differently, tests that cover the same behavior.

Reference inventory: 111 C++ `TEST_CASE`s.

## `test_AudioMidiLoop_audio.cpp` -> `tests/audio_midi_loop_audio.rs`

- `AudioMidiLoop - Audio - Stop` -> `audio_stop`
- `AudioMidiLoop - Audio - Record` -> `audio_record`
- `AudioMidiLoop - Audio - Record Beyond Ext. Buf` -> `audio_record_beyond_external_buffer`
- `AudioMidiLoop - Audio - Record Multiple Target` -> `audio_record_multiple_target`
- `AudioMidiLoop - Audio - Record Multiple Source` -> `audio_record_multiple_source`
- `AudioMidiLoop - Audio - Record Onto Smaller` -> `audio_record_onto_smaller`
- `AudioMidiLoop - Audio - Record Onto Larger` -> `audio_record_onto_larger`
- `AudioMidiLoop - Audio - Playback` -> `audio_playback`
- `AudioMidiLoop - Audio - Playback Multiple Target` -> `audio_playback_multiple_target`
- `AudioMidiLoop - Audio - Playback Shorter Data` -> `audio_playback_shorter_data`
- `AudioMidiLoop - Audio - Playback Wrap` -> `audio_playback_wrap`
- `AudioMidiLoop - Audio - Playback Wrap Longer Data` -> `audio_playback_wrap_longer_data`
- `AudioMidiLoop - Audio - Replace` -> `audio_replace`
- `AudioMidiLoop - Audio - Replace Onto Smaller` -> `audio_replace_onto_smaller`
- `AudioMidiLoop - Audio - Play Dry Through Wet` -> `audio_play_dry_through_wet`
- `AudioMidiLoop - Audio - Record Dry Into Wet` -> `audio_record_dry_into_wet`
- `AudioMidiLoop - Audio - Prerecord` -> `audio_prerecord`
- `AudioMidiLoop - Audio - Preplay` -> `audio_preplay`
- `AudioMidiLoop - Audio - Playback and set to sync` -> `audio_playback_and_set_to_sync`
- `AudioMidiLoop - Audio - Record and set to sync` -> `audio_record_and_set_to_sync`

## `test_AudioMidiLoop_midi.cpp` -> `tests/audio_midi_loop_midi.rs`

- `AudioMidiLoop - Midi - Stop` -> `midi_stop`
- `AudioMidiLoop - Midi - Record` -> `midi_record`
- `AudioMidiLoop - Midi - Record Append Out-of-order` -> `midi_record_append_out_of_order`
- `AudioMidiLoop - Midi - Record multiple source buffers` -> `midi_record_multiple_source_buffers`
- `AudioMidiLoop - Midi - Record onto longer buffer` -> `midi_record_onto_longer_buffer`
- `AudioMidiLoop - Midi - Playback` -> `midi_playback`
- `AudioMidiLoop - Midi - Prerecord` -> `midi_prerecord`
- `AudioMidiLoop - Midi - Preplay` -> `midi_preplay`
- `AudioMidiLoop - Midi - CC State tracking` -> `midi_cc_state_tracking`
- `AudioMidiLoop - Midi - Corner Case - note started before loop boundary` -> `midi_corner_case_note_started_before_loop_boundary`
- `AudioMidiLoop - Midi - Corner Case - note started during pre-play` -> `midi_corner_case_note_started_during_pre_play`
- `AudioMidiLoop - Midi - Corner Case - note pre-recorded but no preplay` -> `midi_corner_case_note_pre_recorded_but_no_preplay`

## `test_BasicLoop.cpp` -> `tests/basic_loop.rs`

- `BasicLoop - Stop` -> `basic_loop_stop`
- `BasicLoop - Record` -> `basic_loop_record`
- `BasicLoop - Planned Transition` -> `basic_loop_planned_transition`
- `BasicLoop - Planned Transition delayed` -> `basic_loop_planned_transition_delayed`
- `BasicLoop - Planned Transitions delayed` -> `basic_loop_planned_transitions_delayed`
- `BasicLoop - Planned Transitions Cancellation 1` -> `basic_loop_planned_transitions_cancellation`
- `BasicLoop - Generate Trigger` -> `basic_loop_generate_trigger`
- `BasicLoop - Generate Trigger on restart` -> `basic_loop_generate_trigger_on_restart`
- `BasicLoop - Playback 0 length` -> `basic_loop_playback_zero_length`

## `test_BufferQueue.cpp` -> `tests/buffer_queue.rs`

- `BufferQueue - Starting state` -> `buffer_queue_starting_state`
- `BufferQueue - Single Buf Full` -> `buffer_queue_single_buf_full`
- `BufferQueue - Single Buf Partial` -> `buffer_queue_single_buf_partial`
- `BufferQueue - Two bufs full` -> `buffer_queue_two_bufs_full`
- `BufferQueue - Two bufs partial` -> `buffer_queue_two_bufs_partial`
- `BufferQueue - drop buffer` -> `buffer_queue_drop_buffer`
- `BufferQueue - drop buffer then change max to drop buffer` -> `buffer_queue_drop_buffer_then_lower_the_limit`
- `BufferQueue - clone then drop` -> `buffer_queue_snapshot_then_drop`

## `test_DummyAudioMidiDriver.cpp` -> `tests/dummy_driver.rs`

- `DummyAudioMidiDriver - Automatic` -> `dummy_driver_automatic`
- `DummyAudioMidiDriver - Controlled` -> `dummy_driver_controlled`
- `DummyAudioMidiDriver - Input port default` -> `dummy_driver_input_port_default`
- `DummyAudioMidiDriver - Input port queue` -> `dummy_driver_input_port_queue`
- `DummyAudioMidiDriver - Input port queue consume multiple` -> `dummy_driver_input_port_queue_consume_multiple`
- `DummyAudioMidiDriver - Input port queue consume combine` -> `dummy_driver_input_port_queue_consume_combine`

## `test_DummyPorts.cpp` -> `tests/dummy_ports.rs`

- `Ports - Dummy Audio In - Properties` -> `dummy_audio_in_properties`
- `Ports - Dummy Audio In - Buffers` -> `dummy_audio_in_buffers`
- `Ports - Dummy Audio In - Queue` -> `dummy_audio_in_queue`
- `Ports - Dummy Audio In - Gain` -> `dummy_audio_in_gain`
- `Ports - Dummy Audio In - Mute` -> `dummy_audio_in_mute`
- `Ports - Dummy Audio In - Peak` -> `dummy_audio_in_peak`
- `Ports - Dummy Audio In - get ringbuffer data` -> `dummy_audio_in_get_ringbuffer_data`
- `Ports - Dummy Audio Out - Properties` -> `dummy_audio_out_properties`
- `Ports - Dummy Audio Out - Buffers` -> `dummy_audio_out_buffers`
- `Ports - Dummy Audio Out - Queue` -> `dummy_audio_out_queue`
- `Ports - Dummy Audio Out - Gain` -> `dummy_audio_out_gain`
- `Ports - Dummy Audio Out - Mute` -> `dummy_audio_out_mute`
- `Ports - Dummy Audio Out - Peak` -> `dummy_audio_out_peak`
- `Ports - Dummy Audio Out - Noop Zero` -> `dummy_audio_out_noop_zero`

## `test_InternalAudioPort.cpp` -> `tests/internal_audio_port.rs`

- `Ports - Internal Audio - Properties` -> `internal_audio_port_properties`
- `Ports - Internal Audio - Gain` -> `internal_audio_port_gain`
- `Ports - Internal Audio - Mute` -> `internal_audio_port_mute`
- `Ports - Internal Audio - Peak` -> `internal_audio_port_peak`
- `Ports - Internal Audio - Noop Zero` -> `internal_audio_port_noop_zero`
- `Ports - Internal Audio - get ringbuffer data` -> `internal_audio_port_get_ringbuffer_data`

## `test_JackPorts.cpp` -> `src/external_audio_port.rs`, `src/external_midi_port.rs`, `tests/midi_port.rs`

The Rust engine has driver-shaped external ports instead of a JACK-test-specific port class. These
are the direct behavioral equivalents.

- `Ports - Jack Audio In - Properties` -> `external_audio_port::tests::access_follows_direction`
- `Ports - Jack Audio In - Gain` -> `external_audio_port::tests::jack_audio_input_gain_and_mute_equivalent`
- `Ports - Jack Audio In - Mute` -> `external_audio_port::tests::jack_audio_input_gain_and_mute_equivalent`
- `Ports - Jack Audio In - Peak` -> `external_audio_port::tests::what_arrived_is_metered_even_when_muted`
- `Ports - Jack Audio In - get ringbuffer data` -> `external_audio_port::tests::jack_audio_input_ringbuffer_snapshot_equivalent`
- `Ports - Jack Audio Out - Properties` -> `external_audio_port::tests::access_follows_direction`
- `Ports - Jack Audio Out - Gain` -> `external_audio_port::tests::gain_and_muting_apply_on_the_way_out`
- `Ports - Jack Audio Out - Mute` -> `external_audio_port::tests::gain_and_muting_apply_on_the_way_out`
- `Ports - Jack Audio Out - Peak` -> `port::tests::input_peak_is_measured_before_gain`, `port::tests::output_peak_reflects_gain`, `port::tests::input_peak_is_measured_even_when_muted`
- `Ports - Jack Audio Out - Noop Zero` -> `external_audio_port::tests::jack_audio_output_starts_next_cycle_silent_equivalent`
- `Ports - Jack Midi In - Properties` -> `external_midi_port::tests::access_follows_direction`
- `Ports - Jack Midi In - Receive` -> `external_midi_port::tests::a_cycles_arrivals_are_visible_then_gone`, `tests/midi_port.rs::midi_port_receive`
- `Ports - Jack Midi In - Mute` -> `external_midi_port::tests::a_muted_input_port_yields_nothing`
- `Ports - Jack Midi In - Message Counter` -> `external_midi_port::tests::jack_midi_input_message_counters_reset_and_muting_equivalent`
- `Ports - Jack Midi In - Note Tracker` -> `tests/midi_port.rs::midi_port_note_tracker`
- `Ports - Jack Midi In - get ringbuffer data` -> `external_midi_port::tests::jack_midi_input_ringbuffer_snapshot_equivalent`
- `Ports - Jack Midi Out - Properties` -> `external_midi_port::tests::access_follows_direction`
- `Ports - Jack Midi Out - Send` -> `tests/midi_port.rs::midi_port_receives_a_run_of_messages_in_order`, `external_midi_port::tests::output_is_ordered_for_the_driver`
- `Ports - Jack Midi Out - Sort` -> `external_midi_port::tests::output_is_ordered_for_the_driver`, `tests/midi_port.rs::midi_port_output_is_sorted_by_time`
- `Ports - Jack Midi Out - Message Counter` -> `external_midi_port::tests::jack_midi_output_message_counters_reset_and_muting_equivalent`
- `Ports - Jack Midi Out - Note Tracker` -> `external_midi_port::tests::an_output_ports_traffic_is_tracked`
- `Ports - Jack Midi Out - Mute` -> `external_midi_port::tests::a_muted_output_port_emits_nothing`

## `test_MidiChannel.cpp` -> `tests/midi_channel.rs`

- `MidiChannel - Set Contents - Indefinite size` -> `midi_channel_set_contents_indefinite_size`

## `test_MidiRingbuffer.cpp` -> `tests/midi_ringbuffer.rs`

- `MidiRingbuffer - Put and increment` -> `midi_ringbuffer_put_and_increment`
- `MidiRingbuffer - Put and truncate` -> `midi_ringbuffer_put_and_truncate`
- `MidiRingbuffer - Put and wrap` -> `midi_ringbuffer_put_and_wrap`
- `MidiRingbuffer - Put and wrap then truncate` -> `midi_ringbuffer_put_and_wrap_then_truncate`
- `MidiRingbuffer - Put then overflow then snapshot` -> `midi_ringbuffer_put_then_overflow_then_snapshot`
- `MidiRingbuffer - Put then truncated snapshot` -> `midi_ringbuffer_put_then_truncated_snapshot`

## `test_MidiStateDiffTracker.cpp` -> `tests/midi_state_diff.rs`

- `MidiStateDiffTracker - channel pressure diff uses correct status byte` -> `channel_pressure_diff_uses_the_correct_status_byte`
- `MidiStateDiffTracker - channel pressure diff is independent from pitch wheel` -> `channel_pressure_is_independent_from_the_pitch_wheel`, `the_pitch_wheel_is_independent_from_channel_pressure`
- `MidiStateDiffTracker - check_channel_pressure uses correct byte` -> `channel_pressure_carries_its_own_channel`

## `test_MidiStorage.cpp` -> `tests/midi_storage.rs`

- `MidiStorage - Round-trip` -> `midi_storage_round_trip`
- `MidiStorage - prepend` -> `midi_storage_prepend`
- `MidiStorage - replace append` -> `midi_storage_replace_append`
- `MidiStorage - wrap around` -> `midi_storage_wrap_around`
