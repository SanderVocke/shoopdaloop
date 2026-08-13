//! One-for-one translation of `unit test test_AudioMidiLoop_midi.cpp`.
//!
//! is a stronger check: it catches places where my reading of the implementation
//! was self-consistent but wrong.
//!

use assert2::{check, let_assert};
use shoop_engine::audio_midi_loop::AudioMidiLoop;
use shoop_engine::basic_loop::SyncSourceState;
use shoop_engine::channel_mode::ChannelMode;
use shoop_engine::loop_mode::LoopMode;
use shoop_engine::midi;
use shoop_engine::midi_storage::MidiStorageElem;

fn msg(time: u32, bytes: &[u8]) -> MidiStorageElem {
    MidiStorageElem::new(time, bytes).expect("valid message")
}

/// Message as (time, payload), for comparing against expected values.
fn as_pair(m: &MidiStorageElem) -> (u32, Vec<u8>) {
    (m.time, m.data().to_vec())
}

fn contents(l: &AudioMidiLoop) -> Vec<(u32, Vec<u8>)> {
    l.midi_channel(0)
        .expect("channel 0")
        .contents()
        .iter()
        .map(as_pair)
        .collect()
}

fn with_time(m: &MidiStorageElem, time: u32) -> (u32, Vec<u8>) {
    (time, m.data().to_vec())
}

/// One `PROC_process` call. Returns what the channel emitted.
///
/// the output comes back, so each call states its own I/O.
fn process(l: &mut AudioMidiLoop, n: u32, input: &[MidiStorageElem]) -> Vec<MidiStorageElem> {
    let midi_in = vec![input.to_vec()];
    let mut midi_out = vec![Vec::new()];
    let_assert!(Ok(()) = l.process(n, &midi_in, &mut midi_out));
    midi_out.remove(0)
}

/// Sets the channel's recording buffer size, as `PROC_set_recording_buffer`.
fn set_recording_buffer(l: &mut AudioMidiLoop, n: u32) {
    l.midi_channel_mut(0)
        .expect("channel 0")
        .set_recording_buffer(n);
}

fn set_playback_buffer(l: &mut AudioMidiLoop, n: u32) {
    l.midi_channel_mut(0)
        .expect("channel 0")
        .set_playback_buffer(n);
}

/// Copies the source loop's state into the follower, as the session does between
fn refresh_sync(follower: &mut AudioMidiLoop, source: &AudioMidiLoop) {
    follower.set_sync_source(Some(source.as_sync_source_state()));
}

fn loop_with_channel(capacity: usize) -> AudioMidiLoop {
    let mut l = AudioMidiLoop::default();
    l.add_midi_channel(capacity, ChannelMode::Direct);
    l
}

#[tracy_nextest_capture::tracy_capture_test]
fn midi_stop() {
    let mut l = loop_with_channel(512);

    check!(l.mode() == LoopMode::Stopped);
    check!(l.next_poi() == None);
    check!(l.length() == 0);
    check!(l.position() == 0);

    process(&mut l, 1000, &[]);

    check!(l.mode() == LoopMode::Stopped);
    check!(l.next_poi() == None);
    check!(l.length() == 0);
    check!(l.position() == 0);
}

#[tracy_nextest_capture::tracy_capture_test]
fn midi_record() {
    let mut l = loop_with_channel(512);

    check!(l.mode() == LoopMode::Stopped);
    check!(l.next_poi() == None);
    check!(l.length() == 0);
    check!(l.position() == 0);

    let source = [
        msg(10, &[0x01, 0x02, 0x03]),
        msg(19, &[0x01, 0x02]),
        msg(20, &[0x00]),
    ];

    l.plan_transition(LoopMode::Recording, Some(0), None);
    set_recording_buffer(&mut l, 512);
    l.trigger(true);
    l.resync_poi();

    check!(l.mode() == LoopMode::Recording);
    check!(l.next_poi() == Some(512)); // end of buffer
    check!(l.length() == 0);
    check!(l.position() == 0);

    process(&mut l, 20, &source);

    check!(l.mode() == LoopMode::Recording);
    check!(l.next_poi() == Some(492)); // end of buffer
    check!(l.length() == 20);
    check!(l.position() == 0);

    // The message at frame 20 is outside the processed window.
    let msgs = contents(&l);
    check!(msgs.len() == 2);
    check!(msgs[0] == as_pair(&source[0]));
    check!(msgs[1] == as_pair(&source[1]));
}

#[tracy_nextest_capture::tracy_capture_test]
fn midi_record_append_out_of_order() {
    let mut l = loop_with_channel(512);

    l.set_mode(LoopMode::Recording);
    l.set_length(100);
    let source = [
        msg(10, &[0x01, 0x02, 0x03]),
        msg(9, &[0x01, 0x02]),
        msg(11, &[0x00]),
    ];
    set_recording_buffer(&mut l, 512);
    l.resync_poi();

    check!(l.mode() == LoopMode::Recording);
    check!(l.next_poi() == Some(512));
    check!(l.length() == 100);
    check!(l.position() == 0);

    process(&mut l, 20, &source);

    check!(l.mode() == LoopMode::Recording);
    check!(l.next_poi() == Some(492)); // end of buffer
    check!(l.length() == 120);
    check!(l.position() == 0);

    // The frame-9 message arrives after the frame-10 one, so storage refuses it
    // rather than reordering.
    let msgs = contents(&l);
    check!(msgs.len() == 2);
    check!(msgs[0] == with_time(&source[0], 110));
    check!(msgs[1] == with_time(&source[2], 111));
}

#[tracy_nextest_capture::tracy_capture_test]
fn midi_record_multiple_source_buffers() {
    let mut l = loop_with_channel(512);

    check!(l.mode() == LoopMode::Stopped);
    check!(l.next_poi() == None);
    check!(l.length() == 0);
    check!(l.position() == 0);

    let buf0 = [
        msg(10, &[0x01, 0x02, 0x03]),
        msg(19, &[0x01, 0x02]),
        msg(20, &[0x00]),
    ];
    let buf1 = [
        msg(21 - 21, &[0x01, 0x02, 0x03]),
        msg(26 - 21, &[0x01, 0x02]),
        msg(29 - 21, &[0x00]),
    ];
    let buf2 = [
        msg(30 - 21 - 9, &[0x01, 0x02, 0x03]),
        msg(30 - 21 - 9, &[0x01, 0x02]),
        msg(31 - 21 - 9, &[0x00]),
        msg(40 - 21 - 9, &[0x00]),
    ];

    l.plan_transition(LoopMode::Recording, Some(0), None);
    set_recording_buffer(&mut l, 21);
    l.trigger(true);
    l.resync_poi();

    check!(l.mode() == LoopMode::Recording);
    check!(l.next_poi().unwrap_or(999) == 21); // end of buffer
    check!(l.length() == 0);
    check!(l.position() == 0);

    process(&mut l, 21, &buf0);

    check!(l.mode() == LoopMode::Recording);
    check!(l.next_poi().unwrap_or(999) == 0); // end of buffer
    check!(l.length() == 21);
    check!(l.position() == 0);

    set_recording_buffer(&mut l, 9);
    l.resync_poi();

    check!(l.mode() == LoopMode::Recording);
    check!(l.next_poi().unwrap_or(999) == 9);
    check!(l.length() == 21);
    check!(l.position() == 0);

    process(&mut l, 9, &buf1);

    check!(l.mode() == LoopMode::Recording);
    check!(l.next_poi().unwrap_or(999) == 0);
    check!(l.length() == 30);
    check!(l.position() == 0);

    set_recording_buffer(&mut l, 100);
    l.resync_poi();

    check!(l.mode() == LoopMode::Recording);
    check!(l.next_poi().unwrap_or(999) == 100);
    check!(l.length() == 30);
    check!(l.position() == 0);

    // Purposefully do not process the last message.
    process(&mut l, 5, &buf2);

    check!(l.mode() == LoopMode::Recording);
    check!(l.next_poi().unwrap_or(999) == 95);
    check!(l.length() == 35);
    check!(l.position() == 0);

    let msgs = contents(&l);
    check!(msgs.len() == 9);
    check!(msgs[0] == as_pair(&buf0[0]));
    check!(msgs[1] == as_pair(&buf0[1]));
    check!(msgs[2] == as_pair(&buf0[2]));
    check!(msgs[3] == with_time(&buf1[0], buf1[0].time + 21));
    check!(msgs[4] == with_time(&buf1[1], buf1[1].time + 21));
    check!(msgs[5] == with_time(&buf1[2], buf1[2].time + 21));
    check!(msgs[6] == with_time(&buf2[0], buf2[0].time + 30));
    check!(msgs[7] == with_time(&buf2[1], buf2[1].time + 30));
    check!(msgs[8] == with_time(&buf2[2], buf2[2].time + 30));
}

#[tracy_nextest_capture::tracy_capture_test]
fn midi_playback() {
    let mut l = loop_with_channel(512);
    let recorded = [
        msg(0, &[0x01]),
        msg(10, &[0x02]),
        msg(21, &[0x03]),
        msg(30, &[0x04]),
        msg(50, &[0x05]),
    ];
    l.midi_channel_mut(0)
        .expect("channel 0")
        .set_contents(&recorded, 100, None);
    l.set_length(100);
    l.set_mode(LoopMode::Playing);
    set_playback_buffer(&mut l, 25);
    set_recording_buffer(&mut l, 25);
    l.resync_poi();

    check!(l.mode() == LoopMode::Playing);
    check!(l.length() == 100);
    check!(l.position() == 0);

    let out = process(&mut l, 25, &[]);

    check!(l.position() == 25);
    // Only the messages inside the first 25 frames sound.
    let played: Vec<(u32, Vec<u8>)> = out.iter().map(as_pair).collect();
    check!(played.len() == 3);
    check!(played[0] == as_pair(&recorded[0]));
    check!(played[1] == as_pair(&recorded[1]));
    check!(played[2] == as_pair(&recorded[2]));
}

#[tracy_nextest_capture::tracy_capture_test]
fn midi_record_onto_longer_buffer() {
    let mut l = loop_with_channel(1024);
    let existing = [
        msg(0, &[0x01]),
        msg(10, &[0x02]),
        msg(21, &[0x03]),
        msg(30, &[0x04]),
        msg(50, &[0x05]),
    ];
    l.midi_channel_mut(0)
        .expect("channel 0")
        .set_contents(&existing, 100, None);
    l.set_mode(LoopMode::Recording);
    l.set_length(25);

    check!(l.mode() == LoopMode::Recording);
    // No recording buffer assigned yet, so the channel reports 0.
    check!(l.next_poi() == Some(0));
    check!(l.length() == 25);
    check!(l.position() == 0);

    let source = [
        msg(1, &[0x01, 0x02, 0x03]),
        msg(2, &[0x01, 0x02]),
        msg(3, &[0x00]),
    ];
    set_recording_buffer(&mut l, 512);
    l.trigger(true);
    l.resync_poi();

    check!(l.mode() == LoopMode::Recording);
    check!(l.next_poi() == Some(512)); // end of buffer
    check!(l.length() == 25);
    check!(l.position() == 0);

    process(&mut l, 20, &source);

    check!(l.mode() == LoopMode::Recording);
    check!(l.next_poi() == Some(492)); // end of buffer
    check!(l.length() == 45);
    check!(l.position() == 0);

    // Recording from 25 truncates the two messages past it, then appends the new
    // ones offset by the existing length plus one.
    let msgs = contents(&l);
    check!(
        msgs == vec![
            as_pair(&existing[0]),
            as_pair(&existing[1]),
            as_pair(&existing[2]),
            with_time(&source[0], 26),
            with_time(&source[1], 27),
            with_time(&source[2], 28),
        ]
    );
}

#[tracy_nextest_capture::tracy_capture_test]
fn midi_pitch_wheel_round_trips_through_a_recording() {
    // engine's constructor agrees with what it recorded and played back.
    let mut l = loop_with_channel(512);
    let wheel = midi::pitch_wheel(3, 1000);
    let source = [msg(1, &wheel)];

    l.set_mode(LoopMode::Recording);
    set_recording_buffer(&mut l, 512);
    l.resync_poi();
    process(&mut l, 8, &source);

    let msgs = contents(&l);
    check!(msgs.len() == 1);
    check!(msgs[0].1 == wheel.to_vec());
}

#[tracy_nextest_capture::tracy_capture_test]
fn midi_prerecord() {
    let mut sync_source = AudioMidiLoop::default();
    sync_source.set_length(100);
    sync_source.plan_transition(LoopMode::Playing, Some(0), None);
    check!(sync_source.predicted_next_trigger_eta().unwrap_or(999) == 100);

    let mut l = loop_with_channel(512);
    // Needed because otherwise the loop would transition immediately.
    l.set_sync_source(Some(SyncSourceState::default()));
    refresh_sync(&mut l, &sync_source);
    l.resync_poi();
    check!(l.predicted_next_trigger_eta().unwrap_or(999) == 100);

    let source = [
        msg(1, &[0x01, 0x02, 0x03]),
        msg(10, &[0x01, 0x02]),
        msg(21, &[0x00]),
        msg(39, &[0x00]),
    ];

    l.plan_transition(LoopMode::Recording, Some(0), None); // not triggered yet
    set_recording_buffer(&mut l, 512);
    l.resync_poi();

    check!(l.mode() == LoopMode::Stopped);
    check!(l.next_poi().unwrap_or(999) == 512); // end of buffer
    check!(l.length() == 0);
    check!(l.position() == 0);

    process(&mut l, 20, &source);

    // Still stopped, but the channel pre-recorded because recording is planned.
    check!(l.mode() == LoopMode::Stopped);
    check!(l.next_poi().unwrap_or(999) == 492); // end of buffer
    check!(l.length() == 0);
    check!(l.position() == 0);

    l.trigger(true);
    l.resync_poi();
    process(&mut l, 20, &source);

    check!(l.mode() == LoopMode::Recording);
    check!(l.next_poi().unwrap_or(999) == 472); // end of buffer
    check!(l.length() == 20);
    check!(l.position() == 0);
    check!(l.predicted_next_trigger_eta().unwrap_or(999) == 80);

    // Thanks to pre-record, every message was captured, including the two from
    // before recording was triggered.
    let msgs = contents(&l);
    check!(msgs.len() == 4);
    check!(l.midi_channel(0).expect("channel 0").start_offset() == 20);
    check!(msgs[0] == as_pair(&source[0]));
    check!(msgs[1] == as_pair(&source[1]));
    check!(msgs[2] == as_pair(&source[2]));
    check!(msgs[3] == as_pair(&source[3]));

    // Advancing the sync source shortens the follower's predicted trigger. The
    // sync source has no channels, so nothing bounds it but its own length.
    sync_source.resync_poi();
    let_assert!(Ok(()) = sync_source.process::<Vec<MidiStorageElem>>(60, &[], &mut []));
    refresh_sync(&mut l, &sync_source);
    l.resync_poi();
    check!(l.predicted_next_trigger_eta().unwrap_or(999) == 40);
}

fn on_channel(out: &[MidiStorageElem], channel: u8) -> Vec<(u32, Vec<u8>)> {
    out.iter()
        .filter(|m| m.data().len() == 3 && (m.data()[0] & 0x0F) == channel)
        .map(as_pair)
        .collect()
}

///
/// and this generates from the rule its own comment states, so the sequence is
/// checked against a stated rule rather than a transcript.
struct StateTracking {
    name: &'static str,
    playback_from: u32,
    playback_to: u32,
    /// Value the restore is expected to revert channel 0's pitch wheel to.
    expect_reset_pitch: u16,
}

#[tracy_nextest_capture::tracy_capture_test]
fn midi_cc_state_tracking() {
    let cases = [
        // Playing from the start reverts to centre, the value the tracker starts at.
        StateTracking {
            name: "pb_from_first_sample",
            playback_from: 0,
            playback_to: 99,
            expect_reset_pitch: 0x2000,
        },
        // Playing from frame 40 reverts to 49: the messages skipped on the way there
        // move the restore target forward, so it is the value just before frame 40.
        StateTracking {
            name: "pb_from_40th_to_50th",
            playback_from: 40,
            playback_to: 50,
            expect_reset_pitch: 49,
        },
    ];

    for case in cases {
        let mut l = loop_with_channel(100_000);

        // A note every 10 ticks over a pitch wheel rising by 1 per tick from 10,
        // split across ten buffers of ten ticks.
        let buffers: Vec<Vec<MidiStorageElem>> = (0..10u32)
            .map(|i| {
                let mut buf = Vec::new();
                for j in 0..10u32 {
                    buf.push(msg(j, &midi::pitch_wheel(0, (10 + i * 10 + j) as u16)));
                    if j == 2 {
                        buf.push(msg(j, &midi::note_on(0, 50, 100)));
                    }
                    if j == 5 {
                        buf.push(msg(j, &midi::note_off(0, 50, 100)));
                    }
                }
                buf
            })
            .collect();

        l.plan_transition(LoopMode::Recording, Some(0), None);
        l.trigger(true);
        l.resync_poi();
        check!(l.mode() == LoopMode::Recording, "{}", case.name);

        for buf in &buffers {
            set_recording_buffer(&mut l, 10);
            l.resync_poi();
            process(&mut l, 10, buf);
        }
        check!(l.length() == 100, "{}", case.name);

        // Stop recording, then move the input while stopped, so the receiver is no
        // longer where the recording assumed it was.
        l.plan_transition(LoopMode::Stopped, Some(0), None);
        l.trigger(true);
        l.resync_poi();
        check!(l.mode() == LoopMode::Stopped, "{}", case.name);

        let another = [
            msg(0, &midi::pitch_wheel(0, 150)),
            // An unrelated pitch wheel change on another channel.
            msg(0, &midi::pitch_wheel(10, 100)),
            // And the hold pedal pressed on a third.
            msg(0, &midi::cc(1, 64, 127)),
        ];
        set_recording_buffer(&mut l, 10);
        l.resync_poi();
        process(&mut l, 10, &another);
        check!(l.length() == 100, "{}", case.name);

        // Play back the requested window.
        let n = case.playback_to - case.playback_from;
        l.plan_transition(LoopMode::Playing, Some(0), None);
        l.trigger(true);
        {
            let ch = l.midi_channel_mut(0).expect("channel 0");
            ch.set_start_offset(0);
            ch.set_pre_play_samples(0);
        }
        l.set_position(case.playback_from);
        set_playback_buffer(&mut l, n);
        l.resync_poi();
        let out = process(&mut l, n, &[]);

        check!(l.position() == case.playback_to, "{}", case.name);
        check!(l.mode() == LoopMode::Playing, "{}", case.name);

        // First the restore, then the recorded sequence at 10 + position.
        let mut expect_channel_0 =
            vec![(0, midi::pitch_wheel(0, case.expect_reset_pitch).to_vec())];
        for p in case.playback_from..case.playback_to {
            let t = p - case.playback_from;
            expect_channel_0.push((t, midi::pitch_wheel(0, (10 + p) as u16).to_vec()));
            if p % 10 == 2 {
                expect_channel_0.push((t, midi::note_on(0, 50, 100).to_vec()));
            }
            if p % 10 == 5 {
                expect_channel_0.push((t, midi::note_off(0, 50, 100).to_vec()));
            }
        }
        check!(on_channel(&out, 0) == expect_channel_0, "{}", case.name);

        // Reset the hold pedal on channel 1: released is a known default, so it is
        // restored even though no pedal message was ever recorded.
        check!(
            on_channel(&out, 1) == vec![(0, midi::cc(1, 64, 0).to_vec())],
            "{}",
            case.name
        );
        // Reset the pitch wheel on channel 10, which the recording never touched.
        check!(
            on_channel(&out, 10) == vec![(0, midi::pitch_wheel(10, 0x2000).to_vec())],
            "{}",
            case.name
        );
    }
}

#[tracy_nextest_capture::tracy_capture_test]
fn midi_corner_case_note_started_before_loop_boundary() {
    // A note that started before recording began but ended inside it must be
    // re-started when playback starts, or the note is lost.
    let mut l = loop_with_channel(512);

    check!(l.mode() == LoopMode::Stopped);
    check!(l.next_poi() == None);
    check!(l.length() == 0);
    check!(l.position() == 0);

    let source = [
        msg(10, &midi::note_on(0, 100, 90)),
        msg(20, &midi::note_off(0, 100, 80)),
        msg(30, &midi::note_on(0, 100, 70)),
        msg(40, &midi::note_off(0, 100, 60)),
    ];
    // Assigned once and never reset, so its frame accounting accumulates across
    set_recording_buffer(&mut l, 512);

    check!(l.next_poi() == None);
    // Nothing is recorded, but the note-on is picked up by state tracking.
    process(&mut l, 12, &source);

    l.plan_transition(LoopMode::Recording, Some(0), None);
    l.trigger(true);
    l.resync_poi();
    check!(l.mode() == LoopMode::Recording);
    check!(l.next_poi() == Some(500)); // end of buffer
    check!(l.length() == 0);
    check!(l.position() == 0);

    // The note-off and the second note-on/off pair are recorded.
    process(&mut l, 30, &source);

    check!(l.mode() == LoopMode::Recording);
    check!(l.next_poi() == Some(470)); // end of buffer
    check!(l.length() == 30);
    check!(l.position() == 0);
    check!(contents(&l).len() == 3);

    l.plan_transition(LoopMode::Stopped, Some(0), None);
    l.trigger(true);
    l.resync_poi();
    check!(l.next_poi() == None);
    process(&mut l, 50, &source);

    // The note-on is emitted purely because a note active when recording started is
    // no longer active now; the rest plays from the recording at 8, 18 and 28.
    set_playback_buffer(&mut l, 512);
    l.plan_transition(LoopMode::Playing, Some(0), None);
    l.trigger(true);
    l.resync_poi();

    let out = process(&mut l, 30, &source);
    let played: Vec<(u32, Vec<u8>)> = out.iter().map(as_pair).collect();
    check!(played.len() == 4);
    check!(played[0] == (0, midi::note_on(0, 100, 90).to_vec()));
    check!(played[1] == (8, midi::note_off(0, 100, 80).to_vec()));
    check!(played[2] == (18, midi::note_on(0, 100, 70).to_vec()));
    check!(played[3] == (28, midi::note_off(0, 100, 60).to_vec()));

    // A second pass round the loop, to prove it works twice. The playback buffer is
    // not reassigned, so times continue from 30.
    let out = process(&mut l, 30, &source);
    let played: Vec<(u32, Vec<u8>)> = out.iter().map(as_pair).collect();
    check!(played.len() == 4);
    check!(played[0] == (30, midi::note_on(0, 100, 90).to_vec()));
    check!(played[1] == (38, midi::note_off(0, 100, 80).to_vec()));
    check!(played[2] == (48, midi::note_on(0, 100, 70).to_vec()));
    check!(played[3] == (58, midi::note_off(0, 100, 60).to_vec()));
}

///
/// Splitting the buffer at points of interest and refreshing the sync snapshot
/// between sub-blocks is what lets a planned transition land on the sync source's
/// cycle boundary instead of at the end of the buffer. The output of every
/// sub-block is concatenated, so times stay relative to the playback buffer.
fn process_synced(
    l: &mut AudioMidiLoop,
    sync_source: &mut AudioMidiLoop,
    n: u32,
    input: &[MidiStorageElem],
) -> Vec<MidiStorageElem> {
    let mut out = Vec::new();
    let mut remaining = n;
    let mut sub_blocks = 0;
    while remaining > 0 {
        sub_blocks += 1;
        assert!(
            sub_blocks <= 16,
            "too many sub-blocks; a point of interest is not being cleared"
        );

        refresh_sync(l, sync_source);
        l.resync_poi();
        sync_source.resync_poi();

        let mut until = remaining;
        if let Some(p) = l.next_poi() {
            until = until.min(p);
        }
        if let Some(p) = sync_source.next_poi() {
            until = until.min(p);
        }

        let midi_in = vec![input.to_vec()];
        let mut midi_out = vec![Vec::new()];
        let_assert!(Ok(()) = l.process(until, &midi_in, &mut midi_out));
        let_assert!(Ok(()) = sync_source.process::<Vec<MidiStorageElem>>(until, &[], &mut []));
        out.append(&mut midi_out[0]);

        l.handle_poi();
        sync_source.handle_poi();
        refresh_sync(l, sync_source);
        l.handle_sync();
        sync_source.handle_sync();

        remaining -= until;
    }
    out
}

#[tracy_nextest_capture::tracy_capture_test]
fn midi_corner_case_note_started_during_pre_play() {
    // A note started during pre-play needs no restore, because it was already sent.
    // But once the loop wraps and there is no pre-play left, it must be inserted.
    let mut sync_source = AudioMidiLoop::default();
    sync_source.set_length(10);
    sync_source.plan_transition(LoopMode::Playing, Some(0), None);
    check!(sync_source.predicted_next_trigger_eta().unwrap_or(999) == 10);

    let mut l = loop_with_channel(512);
    // Needed because otherwise the loop would transition immediately.
    l.set_sync_source(Some(SyncSourceState::default()));
    refresh_sync(&mut l, &sync_source);
    l.resync_poi();
    check!(l.predicted_next_trigger_eta().unwrap_or(999) == 10);

    check!(l.mode() == LoopMode::Stopped);
    check!(l.next_poi() == None);
    check!(l.length() == 0);
    check!(l.position() == 0);

    let source = [
        msg(5, &midi::note_on(0, 100, 90)),
        msg(15, &midi::note_off(0, 100, 80)),
        msg(17, &midi::note_on(0, 100, 70)),
        msg(19, &midi::note_off(0, 100, 60)),
    ];
    set_recording_buffer(&mut l, 512);

    // Recording is planned, so pre-recording starts now and recording proper begins
    // when the sync source wraps, ten frames from here.
    l.plan_transition(LoopMode::Recording, Some(0), None);

    process_synced(&mut l, &mut sync_source, 10, &source);
    check!(l.mode() == LoopMode::Recording);
    check!(l.length() == 0);
    check!(l.position() == 0);

    process_synced(&mut l, &mut sync_source, 10, &source);
    check!(l.mode() == LoopMode::Recording);
    check!(l.length() == 10);
    check!(l.position() == 0);

    // Pre-recording kept the note-on that arrived before recording was triggered, so
    // all four messages are stored at their original times.
    let msgs = contents(&l);
    check!(msgs.len() == 4);
    check!(l.midi_channel(0).expect("channel 0").start_offset() == 10);
    check!(msgs[0] == as_pair(&source[0]));
    check!(msgs[1] == as_pair(&source[1]));
    check!(msgs[2] == as_pair(&source[2]));
    check!(msgs[3] == as_pair(&source[3]));

    l.midi_channel_mut(0)
        .expect("channel 0")
        .set_pre_play_samples(6);

    l.plan_transition(LoopMode::Stopped, None, None);
    l.trigger(true);
    l.resync_poi();
    check!(l.next_poi() == None);
    process_synced(&mut l, &mut sync_source, 50, &source);

    set_playback_buffer(&mut l, 512);
    l.plan_transition(LoopMode::Playing, Some(0), None);

    // The pre-play period, which the note-on falls inside.
    let out = process_synced(&mut l, &mut sync_source, 9, &source);
    check!(l.mode() == LoopMode::Stopped);
    check!(out.len() == 1);
    let more = process_synced(&mut l, &mut sync_source, 1, &source);

    check!(l.mode() == LoopMode::Playing);
    check!(more.is_empty());
    check!(as_pair(&out[0]) == (5, midi::note_on(0, 100, 90).to_vec()));

    // The normal play period: only the note-off and the second note, since the
    // note-on already sounded during pre-play.
    let out = process_synced(&mut l, &mut sync_source, 10, &source);
    let played: Vec<(u32, Vec<u8>)> = out.iter().map(as_pair).collect();
    check!(played.len() == 3);
    check!(played[0] == (15, midi::note_off(0, 100, 80).to_vec()));
    check!(played[1] == (17, midi::note_on(0, 100, 70).to_vec()));
    check!(played[2] == (19, midi::note_off(0, 100, 60).to_vec()));

    // Another cycle. There is no pre-play left, so the note-on is inserted.
    let out = process_synced(&mut l, &mut sync_source, 10, &source);
    let played: Vec<(u32, Vec<u8>)> = out.iter().map(as_pair).collect();
    check!(played.len() == 4);
    check!(played[0] == (20, midi::note_on(0, 100, 90).to_vec()));
    check!(played[1] == (25, midi::note_off(0, 100, 80).to_vec()));
    check!(played[2] == (27, midi::note_on(0, 100, 70).to_vec()));
    check!(played[3] == (29, midi::note_off(0, 100, 60).to_vec()));
}

#[tracy_nextest_capture::tracy_capture_test]
fn midi_corner_case_note_pre_recorded_but_no_preplay() {
    // Same as the pre-play case with no pre-play window. The pre-recorded note-on
    // sits before the loop's start offset, so it never plays from the recording and
    // has to arrive as a state restore instead.
    let mut sync_source = AudioMidiLoop::default();
    sync_source.set_length(10);
    sync_source.plan_transition(LoopMode::Playing, Some(0), None);
    check!(sync_source.predicted_next_trigger_eta().unwrap_or(999) == 10);

    let mut l = loop_with_channel(512);
    l.set_sync_source(Some(SyncSourceState::default()));
    refresh_sync(&mut l, &sync_source);
    l.resync_poi();
    check!(l.predicted_next_trigger_eta().unwrap_or(999) == 10);

    check!(l.mode() == LoopMode::Stopped);
    check!(l.next_poi() == None);
    check!(l.length() == 0);
    check!(l.position() == 0);

    let source = [
        msg(5, &midi::note_on(0, 100, 90)),
        msg(15, &midi::note_off(0, 100, 80)),
        msg(17, &midi::note_on(0, 100, 70)),
        msg(19, &midi::note_off(0, 100, 60)),
    ];
    set_recording_buffer(&mut l, 512);

    l.plan_transition(LoopMode::Recording, Some(0), None);
    process_synced(&mut l, &mut sync_source, 10, &source);
    check!(l.mode() == LoopMode::Recording);
    check!(l.length() == 0);
    check!(l.position() == 0);

    process_synced(&mut l, &mut sync_source, 10, &source);
    check!(l.mode() == LoopMode::Recording);
    check!(l.length() == 10);
    check!(l.position() == 0);

    let msgs = contents(&l);
    check!(msgs.len() == 4);
    check!(l.midi_channel(0).expect("channel 0").start_offset() == 10);
    check!(msgs[0] == as_pair(&source[0]));
    check!(msgs[1] == as_pair(&source[1]));
    check!(msgs[2] == as_pair(&source[2]));
    check!(msgs[3] == as_pair(&source[3]));

    l.plan_transition(LoopMode::Stopped, None, None);
    l.trigger(true);
    l.resync_poi();
    check!(l.next_poi() == None);
    process_synced(&mut l, &mut sync_source, 50, &source);

    set_playback_buffer(&mut l, 512);
    l.plan_transition(LoopMode::Playing, Some(0), None);

    // No pre-play, so nothing sounds before the loop starts.
    let out = process_synced(&mut l, &mut sync_source, 9, &source);
    check!(l.mode() == LoopMode::Stopped);
    check!(out.is_empty());
    let out = process_synced(&mut l, &mut sync_source, 1, &source);
    check!(l.mode() == LoopMode::Playing);
    check!(out.is_empty());

    // The skipped note-on becomes part of the state to restore, so it arrives at the
    // start of playback rather than at its recorded time.
    let out = process_synced(&mut l, &mut sync_source, 10, &source);
    let played: Vec<(u32, Vec<u8>)> = out.iter().map(as_pair).collect();
    check!(played.len() == 4);
    check!(played[0] == (10, midi::note_on(0, 100, 90).to_vec()));
    check!(played[1] == (15, midi::note_off(0, 100, 80).to_vec()));
    check!(played[2] == (17, midi::note_on(0, 100, 70).to_vec()));
    check!(played[3] == (19, midi::note_off(0, 100, 60).to_vec()));

    let out = process_synced(&mut l, &mut sync_source, 10, &source);
    let played: Vec<(u32, Vec<u8>)> = out.iter().map(as_pair).collect();
    check!(played.len() == 4);
    check!(played[0] == (20, midi::note_on(0, 100, 90).to_vec()));
    check!(played[1] == (25, midi::note_off(0, 100, 80).to_vec()));
    check!(played[2] == (27, midi::note_on(0, 100, 70).to_vec()));
    check!(played[3] == (29, midi::note_off(0, 100, 60).to_vec()));
}

#[tracy_nextest_capture::tracy_capture_test]
fn midi_preplay() {
    let mut sync_source = AudioMidiLoop::default();
    sync_source.set_length(100);
    sync_source.plan_transition(LoopMode::Playing, Some(0), None);
    check!(sync_source.predicted_next_trigger_eta().unwrap_or(999) == 100);

    let mut l = loop_with_channel(100_000);
    // Needed because otherwise the loop would transition immediately.
    l.set_sync_source(Some(SyncSourceState::default()));
    refresh_sync(&mut l, &sync_source);
    l.resync_poi();
    check!(l.predicted_next_trigger_eta().unwrap_or(999) == 100);

    // One distinguishable message per frame, so a shifted or dropped message shows up
    // as a wrong velocity rather than a wrong count.
    let recorded: Vec<MidiStorageElem> = (0..256u32)
        .map(|idx| msg(idx, &midi::note_off(0, 100, (idx % 128) as u8)))
        .collect();

    {
        let ch = l.midi_channel_mut(0).expect("channel 0");
        ch.set_contents(&recorded, 256, None);
        ch.set_start_offset(110);
        ch.set_pre_play_samples(90);
    }
    l.set_length(128);

    l.plan_transition(LoopMode::Playing, Some(0), None);
    set_playback_buffer(&mut l, 256);

    // The pre-play period: the loop is still stopped, but the channel is already
    // emitting the 90 frames of material ahead of its start offset.
    let mut out = process_synced(&mut l, &mut sync_source, 99, &[]);
    check!(sync_source.mode() == LoopMode::Playing);
    check!(l.mode() == LoopMode::Stopped);

    out.append(&mut process_synced(&mut l, &mut sync_source, 1, &[]));
    check!(sync_source.mode() == LoopMode::Playing);
    check!(l.mode() == LoopMode::Playing);

    out.append(&mut process_synced(&mut l, &mut sync_source, 28, &[]));
    check!(sync_source.mode() == LoopMode::Playing);
    check!(l.mode() == LoopMode::Playing);

    // Pre-play reaches back 90 frames from start offset 110, so it begins at recorded
    // time 20, which lands at buffer time 10. The first ten frames stay silent.
    check!(out.len() == 118);
    for (i, m) in out.iter().enumerate() {
        let t = 10 + i as u32;
        check!(as_pair(m) == with_time(&recorded[20 + i], t), "message {i}");
    }
}
