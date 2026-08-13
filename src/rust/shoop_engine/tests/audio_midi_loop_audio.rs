//! One-for-one translation of `unit test test_AudioMidiLoop_audio.cpp`.
//!
//! engine is `f32`-only, which keeps them exact anyway: every value these cases use
//! is a small integer, and those are exactly representable.
//!
//! Two mechanical differences:
//!
//!   `PROC_set_recording_buffer` / `PROC_set_playback_buffer`. Here a channel is
//!   told only how large the cycle's buffers are, and the buffers themselves are
//!   passed to `finalize_process`, so each call states its own I/O.
//! - There is no buffer pool. `BufferPool<int>(10, 5, 64)` sets the chunk size from
//!   its third argument, and `add_audio_channel(pool, 10, ...)` passes an initial
//!   buffer count rather than a size. Recording here recycles chunks from a spare
//!   list instead of borrowing them from a shared pool, so only the chunk size
//!   carries over.

use assert2::{check, let_assert};
use shoop_engine::audio_midi_loop::AudioMidiLoop;
use shoop_engine::channel_mode::ChannelMode;
use shoop_engine::loop_mode::LoopMode;
use shoop_engine::midi_storage::MidiStorageElem;

fn ramp(n: usize, offset: usize) -> Vec<f32> {
    (0..n).map(|i| (i + offset) as f32).collect()
}

/// A descending ramp, as `[](pos){ return -(int)pos; }`.
fn neg_ramp(n: usize) -> Vec<f32> {
    (0..n).map(|i| -(i as f32)).collect()
}

/// `PROC_process`. Overrunning a point of interest is asserted separately, so this
/// treats a failure as a broken test rather than an expected outcome.
fn advance(l: &mut AudioMidiLoop, n: u32) {
    let_assert!(Ok(()) = l.process::<Vec<MidiStorageElem>>(n, &[], &mut []));
}

/// `PROC_finalize_process` for one channel, with this cycle's port buffers.
fn finalize(l: &mut AudioMidiLoop, idx: usize, src: &[f32], dst: &mut [f32]) {
    l.audio_channel_mut(idx)
        .expect("channel")
        .finalize_process(src, dst);
}

fn set_recording_buffers(l: &mut AudioMidiLoop, n_channels: usize, size: usize) {
    for i in 0..n_channels {
        l.audio_channel_mut(i)
            .expect("channel")
            .set_recording_buffer_size(size);
    }
}

fn channel_modes(l: &AudioMidiLoop, n: usize) -> Vec<ChannelMode> {
    (0..n)
        .map(|i| l.audio_channel(i).expect("channel").mode())
        .collect()
}

fn channel_data(l: &AudioMidiLoop, idx: usize) -> Vec<f32> {
    l.audio_channel(idx).expect("channel").data()
}

#[tracy_nextest_capture::tracy_capture_test]
fn audio_stop() {
    let mut l = AudioMidiLoop::default();
    l.add_audio_channel(256, ChannelMode::Direct);

    check!(l.mode() == LoopMode::Stopped);
    check!(l.next_poi() == None);
    check!(l.length() == 0);
    check!(l.position() == 0);

    advance(&mut l, 1000);
    finalize(&mut l, 0, &[], &mut []);

    check!(l.mode() == LoopMode::Stopped);
    check!(l.next_poi() == None);
    check!(l.length() == 0);
    check!(l.position() == 0);
}

#[tracy_nextest_capture::tracy_capture_test]
fn audio_record() {
    let mut l = AudioMidiLoop::default();
    l.add_audio_channel(64, ChannelMode::Direct);
    l.add_audio_channel(64, ChannelMode::Dry);
    l.add_audio_channel(64, ChannelMode::Wet);

    let source = ramp(512, 0);
    l.plan_transition(LoopMode::Recording, Some(0), None);
    set_recording_buffers(&mut l, 3, source.len());
    l.trigger(true);
    l.resync_poi();

    check!(l.mode() == LoopMode::Recording);
    check!(l.next_poi() == Some(512)); // end of buffer
    check!(l.length() == 0);
    check!(l.position() == 0);

    advance(&mut l, 20);
    for idx in 0..3 {
        finalize(&mut l, idx, &source, &mut []);
    }

    check!(l.mode() == LoopMode::Recording);
    check!(l.next_poi() == Some(492)); // end of buffer
    check!(l.length() == 20);
    check!(l.position() == 0);

    // Every mode records its input identically; they differ only on playback.
    for idx in 0..3 {
        check!(
            channel_data(&l, idx)[..20] == ramp(20, 0)[..],
            "channel {idx}"
        );
    }
}

/// scheduler contract violation, not a runtime condition a caller can handle, so
#[test]
#[should_panic(expected = "beyond its next POI")]
fn audio_record_beyond_external_buffer() {
    let mut l = AudioMidiLoop::default();
    l.add_audio_channel(256, ChannelMode::Direct);

    // No room to record from, which puts the point of interest at 0.
    l.audio_channel_mut(0)
        .expect("channel")
        .set_recording_buffer_size(0);
    l.plan_transition(LoopMode::Recording, Some(0), None);
    l.trigger(true);
    l.resync_poi();

    check!(l.next_poi() == Some(0));
    advance(&mut l, 20);
}

#[tracy_nextest_capture::tracy_capture_test]
fn audio_record_multiple_target() {
    let mut l = AudioMidiLoop::default();
    l.add_audio_channel(64, ChannelMode::Direct);

    let source = ramp(512, 0);
    l.plan_transition(LoopMode::Recording, Some(0), None);
    set_recording_buffers(&mut l, 1, source.len());
    l.trigger(true);
    l.resync_poi();

    check!(l.mode() == LoopMode::Recording);
    check!(l.next_poi() == Some(512)); // end of buffer
    check!(l.length() == 0);
    check!(l.position() == 0);

    // Eight chunks' worth in one cycle, so storage has to grow mid-recording.
    advance(&mut l, 512);
    finalize(&mut l, 0, &source, &mut []);

    check!(l.mode() == LoopMode::Recording);
    check!(l.next_poi() == Some(0)); // end of buffer
    check!(l.length() == 512);
    check!(l.position() == 0);
    check!(channel_data(&l, 0)[..512] == source[..]);
}

#[tracy_nextest_capture::tracy_capture_test]
fn audio_record_multiple_source() {
    let mut l = AudioMidiLoop::default();
    l.add_audio_channel(64, ChannelMode::Direct);

    let mut source = ramp(32, 0);
    l.plan_transition(LoopMode::Recording, Some(0), None);
    l.trigger(true);
    set_recording_buffers(&mut l, 1, source.len());
    l.resync_poi();

    check!(l.mode() == LoopMode::Recording);
    check!(l.next_poi() == Some(32)); // end of buffer
    check!(l.length() == 0);
    check!(l.position() == 0);

    // A fresh 32-frame input buffer each cycle, continuing the ramp, so one
    // recording spans many source buffers.
    let mut processed = 0usize;
    while processed < 512 {
        check!(l.next_poi() == Some(32));
        advance(&mut l, 32);
        finalize(&mut l, 0, &source, &mut []);
        processed += 32;
        source = ramp(32, processed);
        set_recording_buffers(&mut l, 1, source.len());
        l.resync_poi();
        l.handle_poi();
    }

    check!(l.mode() == LoopMode::Recording);
    check!(l.next_poi() == Some(32)); // end of buffer
    check!(l.length() == 512);
    check!(l.position() == 0);
    check!(channel_data(&l, 0)[..512] == ramp(512, 0)[..]);
}

#[tracy_nextest_capture::tracy_capture_test]
fn audio_record_onto_smaller() {
    let mut l = AudioMidiLoop::default();
    l.add_audio_channel(64, ChannelMode::Direct);
    l.audio_channel_mut(0)
        .expect("channel")
        .load_data(&neg_ramp(64));
    l.plan_transition(LoopMode::Recording, Some(0), None);
    l.trigger(true);
    // A loop longer than the data it holds, so recording appends past the end.
    l.set_length(128);

    let source = ramp(512, 0);
    set_recording_buffers(&mut l, 1, source.len());
    l.resync_poi();

    check!(l.mode() == LoopMode::Recording);
    check!(l.next_poi() == Some(512)); // end of buffer
    check!(l.length() == 128);
    check!(l.position() == 0);

    advance(&mut l, 20);
    finalize(&mut l, 0, &source, &mut []);

    check!(l.mode() == LoopMode::Recording);
    check!(l.next_poi() == Some(492)); // end of buffer
    check!(l.length() == 148);
    check!(l.position() == 0);

    let data = channel_data(&l, 0);
    // The existing data is untouched.
    check!(data[..64] == neg_ramp(64)[..]);
    // The gap between it and the loop end reads as silence.
    check!(data[64..128].iter().all(|&v| v == 0.0));
    // And the recording lands at the end.
    check!(data[128..148] == ramp(20, 0)[..]);
}

#[tracy_nextest_capture::tracy_capture_test]
fn audio_record_onto_larger() {
    let mut l = AudioMidiLoop::default();
    l.add_audio_channel(64, ChannelMode::Direct);
    l.audio_channel_mut(0)
        .expect("channel")
        .load_data(&neg_ramp(128));
    l.plan_transition(LoopMode::Recording, Some(0), None);
    l.trigger(true);
    // A loop shorter than the data it holds, so recording overwrites from there.
    l.set_length(64);

    let source = ramp(512, 0);
    set_recording_buffers(&mut l, 1, source.len());
    l.resync_poi();

    check!(l.mode() == LoopMode::Recording);
    check!(l.next_poi() == Some(512)); // end of buffer
    check!(l.length() == 64);
    check!(l.position() == 0);

    advance(&mut l, 64);
    finalize(&mut l, 0, &source, &mut []);

    check!(l.mode() == LoopMode::Recording);
    check!(l.next_poi() == Some(448)); // end of buffer
    check!(l.length() == 128);
    check!(l.position() == 0);

    let data = channel_data(&l, 0);
    // Up to the loop end the old data survives.
    check!(data[..64] == neg_ramp(64)[..]);
    // Past it, the recording has overwritten what was there.
    check!(data[64..128] == ramp(64, 0)[..]);
}

#[tracy_nextest_capture::tracy_capture_test]
fn audio_playback() {
    let mut l = AudioMidiLoop::default();
    l.add_audio_channel(64, ChannelMode::Direct);
    l.add_audio_channel(64, ChannelMode::Dry);
    l.add_audio_channel(64, ChannelMode::Wet);

    let data = ramp(64, 0);
    for idx in 0..3 {
        l.audio_channel_mut(idx).expect("channel").load_data(&data);
    }
    l.set_length(64);
    let mut play: Vec<Vec<f32>> = (0..3).map(|_| vec![0.0; 64]).collect();

    l.plan_transition(LoopMode::Playing, Some(0), None);
    for idx in 0..3 {
        l.audio_channel_mut(idx)
            .expect("channel")
            .set_playback_buffer_size(64);
    }
    l.trigger(true);
    l.resync_poi();

    check!(l.mode() == LoopMode::Playing);
    check!(l.next_poi() == Some(64)); // end of buffer
    check!(l.position() == 0);
    check!(l.length() == 64);

    advance(&mut l, 20);
    for (idx, dst) in play.iter_mut().enumerate() {
        l.audio_channel_mut(idx)
            .expect("channel")
            .finalize_process(&[], dst);
    }

    check!(l.mode() == LoopMode::Playing);
    check!(l.next_poi() == Some(44)); // end of buffer
    check!(l.length() == 64);
    check!(l.position() == 20);

    check!(play[0][..20] == data[..20]);
    // Dry is idle while playing: its material only reaches an output through a wet
    // channel, so nothing is emitted here.
    check!(play[1][..20].iter().all(|&v| v == 0.0));
    check!(play[2][..20] == data[..20]);
}

#[tracy_nextest_capture::tracy_capture_test]
fn audio_playback_multiple_target() {
    let mut l = AudioMidiLoop::default();
    l.add_audio_channel(64, ChannelMode::Direct);

    let data = ramp(512, 0);
    l.audio_channel_mut(0).expect("channel").load_data(&data);
    l.set_length(512);
    l.plan_transition(LoopMode::Playing, Some(0), None);
    l.trigger(true);
    l.resync_poi();

    check!(l.mode() == LoopMode::Playing);
    check!(l.position() == 0);
    check!(l.length() == 512);

    // `play_buf.data() + processed`.
    let mut play = vec![0.0f32; 512];
    let mut processed = 0usize;
    while processed < 512 {
        l.audio_channel_mut(0)
            .expect("channel")
            .set_playback_buffer_size(64);
        l.resync_poi();
        check!(l.next_poi() == Some(64)); // end of buffer
        advance(&mut l, 64);
        finalize(&mut l, 0, &[], &mut play[processed..processed + 64]);
        processed += 64;
    }

    check!(play == data);
}

#[tracy_nextest_capture::tracy_capture_test]
fn audio_playback_shorter_data() {
    let mut l = AudioMidiLoop::default();
    l.add_audio_channel(64, ChannelMode::Direct);

    let data = ramp(32, 0);
    l.audio_channel_mut(0).expect("channel").load_data(&data);
    // A loop longer than its data, so playback runs off the end into silence.
    l.set_length(64);
    let mut play = vec![0.0f32; 64];

    l.plan_transition(LoopMode::Playing, Some(0), None);
    l.audio_channel_mut(0)
        .expect("channel")
        .set_playback_buffer_size(64);
    l.trigger(true);
    l.resync_poi();

    check!(l.mode() == LoopMode::Playing);
    check!(l.next_poi() == Some(64)); // end of buffer
    check!(l.position() == 0);
    check!(l.length() == 64);

    advance(&mut l, 62);
    finalize(&mut l, 0, &[], &mut play);

    check!(l.mode() == LoopMode::Playing);
    check!(l.next_poi() == Some(2)); // end of buffer
    check!(l.length() == 64);
    check!(l.position() == 62);

    check!(play[..32] == data[..]);
    check!(play[32..62].iter().all(|&v| v == 0.0));
}

#[tracy_nextest_capture::tracy_capture_test]
fn audio_playback_wrap() {
    let mut l = AudioMidiLoop::default();
    l.add_audio_channel(64, ChannelMode::Direct);

    let data = ramp(64, 0);
    l.audio_channel_mut(0).expect("channel").load_data(&data);
    l.set_length(64);
    l.set_mode(LoopMode::Playing);
    l.set_position(48);
    // Assigned once, so the two cycles below fill successive parts of it.
    let mut play = vec![0.0f32; 64];

    check!(l.mode() == LoopMode::Playing);
    l.audio_channel_mut(0)
        .expect("channel")
        .set_playback_buffer_size(64);
    l.resync_poi();

    check!(l.mode() == LoopMode::Playing);
    check!(l.next_poi() == Some(64 - 48)); // end of loop
    check!(l.position() == 48);
    check!(l.length() == 64);

    advance(&mut l, 16);
    l.handle_poi();
    finalize(&mut l, 0, &[], &mut play);

    check!(l.mode() == LoopMode::Playing);
    check!(l.next_poi() == Some(48)); // end of buffer
    check!(l.length() == 64);
    check!(l.position() == 0);

    advance(&mut l, 48);
    l.handle_poi();
    finalize(&mut l, 0, &[], &mut play);

    check!(l.mode() == LoopMode::Playing);
    check!(l.next_poi() == Some(0)); // end of buffer
    check!(l.length() == 64);
    check!(l.position() == 48);

    // The tail of the loop, then the start of it again.
    check!(play[..16] == data[48..]);
    check!(play[16..] == data[..48]);
}

#[tracy_nextest_capture::tracy_capture_test]
fn audio_playback_wrap_longer_data() {
    let mut l = AudioMidiLoop::default();
    l.add_audio_channel(64, ChannelMode::Direct);

    // Twice as much data as the loop plays, so wrapping must go back to 0 rather
    // than run on into what follows.
    let data = ramp(128, 0);
    l.audio_channel_mut(0).expect("channel").load_data(&data);
    l.set_length(64);
    l.set_mode(LoopMode::Playing);
    l.set_position(48);
    let mut play = vec![0.0f32; 64];

    l.audio_channel_mut(0)
        .expect("channel")
        .set_playback_buffer_size(64);
    l.resync_poi();

    check!(l.mode() == LoopMode::Playing);
    check!(l.next_poi() == Some(64 - 48)); // end of loop
    check!(l.position() == 48);
    check!(l.length() == 64);

    advance(&mut l, 16);
    l.handle_poi();
    finalize(&mut l, 0, &[], &mut play);

    check!(l.mode() == LoopMode::Playing);
    check!(l.next_poi() == Some(48)); // end of buffer
    check!(l.length() == 64);
    check!(l.position() == 0);

    advance(&mut l, 48);
    l.handle_poi();
    finalize(&mut l, 0, &[], &mut play);

    check!(l.mode() == LoopMode::Playing);
    check!(l.next_poi() == Some(0)); // end of buffer
    check!(l.length() == 64);
    check!(l.position() == 48);

    check!(play[..16] == data[48..64]);
    check!(play[16..] == data[..48]);
}

#[tracy_nextest_capture::tracy_capture_test]
fn audio_replace() {
    let mut l = AudioMidiLoop::default();
    l.add_audio_channel(64, ChannelMode::Direct);
    l.add_audio_channel(64, ChannelMode::Dry);
    l.add_audio_channel(64, ChannelMode::Wet);

    let data = ramp(64, 0);
    for idx in 0..3 {
        l.audio_channel_mut(idx).expect("channel").load_data(&data);
    }
    l.set_length(64);
    l.set_mode(LoopMode::Replacing);
    l.set_position(16);

    let input = neg_ramp(64);
    set_recording_buffers(&mut l, 3, input.len());
    l.resync_poi();

    check!(l.mode() == LoopMode::Replacing);
    check!(l.next_poi() == Some(64 - 16)); // end of loop
    check!(l.position() == 16);
    check!(l.length() == 64);

    advance(&mut l, 32);
    for idx in 0..3 {
        finalize(&mut l, idx, &input, &mut []);
    }

    check!(l.mode() == LoopMode::Replacing);
    check!(l.next_poi() == Some(64 - 32 - 16)); // end of loop
    check!(l.length() == 64);
    check!(l.position() == 16 + 32);

    // Replacing overwrites in place: only the window that was played over changes,
    // and every mode does it the same way.
    for idx in 0..3 {
        let got = channel_data(&l, idx);
        check!(got[..16] == data[..16], "channel {idx}");
        check!(got[16..48] == input[..32], "channel {idx}");
        check!(got[48..64] == data[48..64], "channel {idx}");
    }
}

#[tracy_nextest_capture::tracy_capture_test]
fn audio_replace_onto_smaller() {
    let mut l = AudioMidiLoop::default();
    l.add_audio_channel(64, ChannelMode::Direct);

    let data = ramp(64, 0);
    l.audio_channel_mut(0).expect("channel").load_data(&data);
    l.set_length(64);
    l.set_mode(LoopMode::Replacing);
    l.set_position(48);

    let input = neg_ramp(64);
    set_recording_buffers(&mut l, 1, input.len());
    l.resync_poi();

    check!(l.mode() == LoopMode::Replacing);
    check!(l.next_poi() == Some(64 - 48)); // end of loop
    check!(l.position() == 48);
    check!(l.length() == 64);

    // Replacing across the loop end, so the second half of the input wraps to the
    advance(&mut l, 16);
    l.handle_poi();
    l.resync_poi();
    advance(&mut l, 16);
    finalize(&mut l, 0, &input, &mut []);

    check!(l.mode() == LoopMode::Replacing);
    check!(l.next_poi() == Some(48));
    check!(l.length() == 64); // wrapped around, not extended
    check!(l.position() == 16);

    let got = channel_data(&l, 0);
    // The part of the input that landed after the wrap.
    check!(got[..16] == input[16..32]);
    check!(got[16..48] == data[16..48]);
    // And the part before it.
    check!(got[48..64] == input[..16]);
}

#[tracy_nextest_capture::tracy_capture_test]
fn audio_play_dry_through_wet() {
    let mut l = AudioMidiLoop::default();
    l.add_audio_channel(64, ChannelMode::Direct);
    l.add_audio_channel(64, ChannelMode::Dry);
    l.add_audio_channel(64, ChannelMode::Wet);

    let data = ramp(64, 0);
    for idx in 0..3 {
        l.audio_channel_mut(idx).expect("channel").load_data(&data);
    }
    l.set_length(64);
    let mut play: Vec<Vec<f32>> = (0..3).map(|_| vec![0.0; 64]).collect();

    l.plan_transition(LoopMode::PlayingDryThroughWet, Some(0), None);
    for idx in 0..3 {
        l.audio_channel_mut(idx)
            .expect("channel")
            .set_playback_buffer_size(64);
    }
    l.trigger(true);
    l.resync_poi();

    check!(l.mode() == LoopMode::PlayingDryThroughWet);
    check!(l.next_poi() == Some(64)); // end of buffer
    check!(l.position() == 0);
    check!(l.length() == 64);

    advance(&mut l, 20);
    for (idx, dst) in play.iter_mut().enumerate() {
        l.audio_channel_mut(idx)
            .expect("channel")
            .finalize_process(&[], dst);
    }

    check!(l.mode() == LoopMode::PlayingDryThroughWet);
    check!(l.next_poi() == Some(44)); // end of buffer
    check!(l.length() == 64);
    check!(l.position() == 20);

    check!(play[0][..20] == data[..20]);
    // The mirror image of plain playback: dry plays out so it can be re-processed,
    // and wet stays quiet so the re-processed signal is what gets heard.
    check!(play[1][..20] == data[..20]);
    check!(play[2][..20].iter().all(|&v| v == 0.0));
}

///
/// end, so the queued copies accumulate across sub-blocks.
fn advance_synced(l: &mut AudioMidiLoop, sync_source: &mut AudioMidiLoop, n: u32) {
    let mut remaining = n;
    let mut sub_blocks = 0;
    while remaining > 0 {
        sub_blocks += 1;
        assert!(
            sub_blocks <= 32,
            "too many sub-blocks; a point of interest is not being cleared"
        );

        l.set_sync_source(Some(sync_source.as_sync_source_state()));
        l.resync_poi();
        sync_source.resync_poi();

        let mut until = remaining;
        if let Some(p) = l.next_poi() {
            until = until.min(p);
        }
        if let Some(p) = sync_source.next_poi() {
            until = until.min(p);
        }

        advance(l, until);
        advance(sync_source, until);

        l.handle_poi();
        sync_source.handle_poi();
        l.set_sync_source(Some(sync_source.as_sync_source_state()));
        l.handle_sync();
        sync_source.handle_sync();

        remaining -= until;
    }
}

/// A sync source of the given length, already playing.
fn playing_sync_source(length: u32) -> AudioMidiLoop {
    let mut s = AudioMidiLoop::default();
    s.set_length(length);
    s.plan_transition(LoopMode::Playing, Some(0), None);
    s
}

/// A follower attached to `source`, with three channels one of each mode.
fn follower_with_three_channels(source: &AudioMidiLoop, chunk: usize) -> AudioMidiLoop {
    let mut l = AudioMidiLoop::default();
    // Needed because otherwise the loop would transition immediately.
    l.set_sync_source(Some(source.as_sync_source_state()));
    l.resync_poi();
    l.add_audio_channel(chunk, ChannelMode::Direct);
    l.add_audio_channel(chunk, ChannelMode::Dry);
    l.add_audio_channel(chunk, ChannelMode::Wet);
    l
}

#[tracy_nextest_capture::tracy_capture_test]
fn audio_record_dry_into_wet() {
    let mut l = AudioMidiLoop::default();
    l.add_audio_channel(64, ChannelMode::Direct);
    l.add_audio_channel(64, ChannelMode::Dry);
    l.add_audio_channel(64, ChannelMode::Wet);

    let data = ramp(64, 0);
    for idx in 0..3 {
        l.audio_channel_mut(idx).expect("channel").load_data(&data);
    }
    l.set_length(64);
    l.set_mode(LoopMode::RecordingDryIntoWet);
    l.set_position(16);

    let input = neg_ramp(64);
    set_recording_buffers(&mut l, 3, input.len());
    let mut out: Vec<Vec<f32>> = (0..3).map(|_| vec![0.0; 32]).collect();
    for idx in 0..3 {
        l.audio_channel_mut(idx)
            .expect("channel")
            .set_playback_buffer_size(32);
    }
    l.resync_poi();

    check!(l.mode() == LoopMode::RecordingDryIntoWet);
    check!(l.next_poi() == Some(32)); // end of playback buffer
    check!(l.position() == 16);
    check!(l.length() == 64);

    advance(&mut l, 32);
    for (idx, dst) in out.iter_mut().enumerate() {
        l.audio_channel_mut(idx)
            .expect("channel")
            .finalize_process(&input, dst);
    }

    check!(l.mode() == LoopMode::RecordingDryIntoWet);
    check!(l.next_poi() == Some(0)); // end of loop
    check!(l.length() == 64);
    check!(l.position() == 16 + 32);

    // Direct and wet replace what they had; dry plays out so it can be re-processed
    // into the wet channel, which is the whole point of the mode.
    for idx in [0, 2] {
        let got = channel_data(&l, idx);
        check!(got[..16] == data[..16], "channel {idx}");
        check!(got[16..48] == input[..32], "channel {idx}");
        check!(got[48..64] == data[48..64], "channel {idx}");
        check!(out[idx].iter().all(|&v| v == 0.0), "channel {idx}");
    }

    check!(channel_data(&l, 1)[..64] == data[..]);
    check!(out[1] == ramp(32, 16));
}

#[tracy_nextest_capture::tracy_capture_test]
fn audio_prerecord() {
    let mut sync_source = playing_sync_source(100);
    check!(sync_source.predicted_next_trigger_eta().unwrap_or(999) == 100);

    let mut l = follower_with_three_channels(&sync_source, 64);
    check!(l.predicted_next_trigger_eta().unwrap_or(999) == 100);

    let source = ramp(512, 0);
    l.plan_transition(LoopMode::Recording, Some(0), None); // not triggered yet
    set_recording_buffers(&mut l, 3, source.len());
    l.resync_poi();

    check!(l.mode() == LoopMode::Stopped);
    check!(l.next_poi() == Some(512)); // end of buffer
    check!(l.length() == 0);
    check!(l.position() == 0);

    advance(&mut l, 20);
    for idx in 0..3 {
        finalize(&mut l, idx, &source, &mut []);
    }

    // Still stopped, but the channels pre-recorded because recording is planned.
    check!(l.mode() == LoopMode::Stopped);
    check!(l.next_poi() == Some(492)); // end of buffer
    check!(l.length() == 0);
    check!(l.position() == 0);

    l.trigger(true);
    l.resync_poi();

    advance(&mut l, 20);
    for idx in 0..3 {
        finalize(&mut l, idx, &source, &mut []);
    }

    check!(l.mode() == LoopMode::Recording);
    check!(l.next_poi() == Some(472)); // end of buffer
    check!(l.length() == 20);
    check!(l.position() == 0);
    check!(l.predicted_next_trigger_eta().unwrap_or(999) == 80);

    // The pre-recorded material was adopted, so the channel holds 40 samples with
    // the loop starting 20 in.
    for idx in 0..3 {
        let ch = l.audio_channel(idx).expect("channel");
        check!(ch.start_offset() == 20, "channel {idx}");
        check!(ch.length() == 40, "channel {idx}");
        check!(
            channel_data(&l, idx)[..40] == ramp(40, 0)[..],
            "channel {idx}"
        );
    }

    advance(&mut sync_source, 60);
    l.set_sync_source(Some(sync_source.as_sync_source_state()));
    l.resync_poi();
    check!(l.predicted_next_trigger_eta().unwrap_or(999) == 40);
}

#[tracy_nextest_capture::tracy_capture_test]
fn audio_preplay() {
    let mut sync_source = playing_sync_source(100);
    check!(sync_source.predicted_next_trigger_eta().unwrap_or(999) == 100);

    let mut l = follower_with_three_channels(&sync_source, 64);
    check!(l.predicted_next_trigger_eta().unwrap_or(999) == 100);

    let data = ramp(256, 0);
    for idx in 0..3 {
        let ch = l.audio_channel_mut(idx).expect("channel");
        ch.load_data(&data);
        ch.set_start_offset(110);
        ch.set_pre_play_samples(90);
    }
    l.set_length(128);
    let mut play: Vec<Vec<f32>> = (0..3).map(|_| vec![0.0; 128]).collect();

    l.plan_transition(LoopMode::Playing, Some(0), None);
    for idx in 0..3 {
        l.audio_channel_mut(idx)
            .expect("channel")
            .set_playback_buffer_size(128);
    }

    // The pre-play period: the loop is still stopped while the channels emit the 90
    // frames ahead of their start offset.
    advance_synced(&mut l, &mut sync_source, 99);
    check!(sync_source.mode() == LoopMode::Playing);
    check!(l.mode() == LoopMode::Stopped);

    advance_synced(&mut l, &mut sync_source, 1);
    check!(sync_source.mode() == LoopMode::Playing);
    check!(l.mode() == LoopMode::Playing);

    advance_synced(&mut l, &mut sync_source, 28);
    check!(sync_source.mode() == LoopMode::Playing);
    check!(l.mode() == LoopMode::Playing);

    for (idx, dst) in play.iter_mut().enumerate() {
        l.audio_channel_mut(idx)
            .expect("channel")
            .finalize_process(&[], dst);
    }

    let modes = channel_modes(&l, 3);
    for (idx, (buf, mode)) in play.iter().zip(&modes).enumerate() {
        if *mode == ChannelMode::Dry {
            // Dry never plays out on its own.
            check!(buf.iter().all(|&v| v == 0.0), "channel {idx}");
        } else {
            // Silent until pre-play reaches back far enough, then continuous.
            check!(buf[..10].iter().all(|&v| v == 0.0), "channel {idx}");
            check!(buf[10..] == ramp(118, 20)[..], "channel {idx}");
        }
    }
}

#[tracy_nextest_capture::tracy_capture_test]
fn audio_playback_and_set_to_sync() {
    let mut sync_source = playing_sync_source(30);
    check!(sync_source.predicted_next_trigger_eta().unwrap_or(999) == 30);

    let mut l = follower_with_three_channels(&sync_source, 64);
    check!(l.predicted_next_trigger_eta().unwrap_or(999) == 30);

    let data = ramp(256, 0);
    for idx in 0..3 {
        let ch = l.audio_channel_mut(idx).expect("channel");
        ch.load_data(&data);
        ch.set_start_offset(110);
    }
    l.set_length(128);
    let mut play: Vec<Vec<f32>> = (0..3).map(|_| vec![0.0; 128]).collect();
    for idx in 0..3 {
        l.audio_channel_mut(idx)
            .expect("channel")
            .set_playback_buffer_size(128);
    }

    // One full sync cycle plus ten frames, all while stopped.
    advance_synced(&mut l, &mut sync_source, 40);
    check!(sync_source.position() == 10);
    check!(l.position() == 0);
    check!(l.mode() == LoopMode::Stopped);

    // Aligning to the sync source's cycle 1 takes effect at once, and puts the
    // position where it would have been had the loop been playing all along.
    l.plan_transition(LoopMode::Playing, None, Some(1));
    check!(sync_source.position() == 10);
    check!(l.position() == 40);
    check!(l.mode() == LoopMode::Playing);

    advance_synced(&mut l, &mut sync_source, 4);
    for (idx, dst) in play.iter_mut().enumerate() {
        l.audio_channel_mut(idx)
            .expect("channel")
            .finalize_process(&[], dst);
    }

    let modes = channel_modes(&l, 3);
    for (idx, (buf, mode)) in play.iter().zip(&modes).enumerate() {
        check!(buf[..40].iter().all(|&v| v == 0.0), "channel {idx}");
        let expected: Vec<f32> = if *mode == ChannelMode::Dry {
            vec![0.0; 4]
        } else {
            ramp(4, 150)
        };
        check!(buf[40..44] == expected[..], "channel {idx}");
    }
}

#[tracy_nextest_capture::tracy_capture_test]
fn audio_record_and_set_to_sync() {
    let mut sync_source = playing_sync_source(30);
    check!(sync_source.predicted_next_trigger_eta().unwrap_or(999) == 30);

    let mut l = follower_with_three_channels(&sync_source, 64);
    check!(l.predicted_next_trigger_eta().unwrap_or(999) == 30);

    let data = ramp(256, 0);
    set_recording_buffers(&mut l, 3, 256);
    l.set_length(128);
    for idx in 0..3 {
        l.audio_channel_mut(idx)
            .expect("channel")
            .set_playback_buffer_size(128);
    }

    advance_synced(&mut l, &mut sync_source, 40);
    check!(sync_source.position() == 10);
    check!(l.position() == 0);
    check!(l.mode() == LoopMode::Stopped);

    l.plan_transition(LoopMode::Recording, None, Some(1));
    check!(sync_source.position() == 10);
    check!(l.position() == 0);
    check!(l.length() == 40);
    check!(l.mode() == LoopMode::Recording);

    advance_synced(&mut l, &mut sync_source, 4);
    let mut sink = vec![0.0f32; 128];
    for idx in 0..3 {
        l.audio_channel_mut(idx)
            .expect("channel")
            .finalize_process(&data, &mut sink);
    }

    // The recording buffer's cursor advanced through the stopped frames too, so the
    // four recorded samples come from offset 40 rather than the start.
    for idx in 0..3 {
        let got = channel_data(&l, idx);
        check!(got.len() == 44, "channel {idx}");
        check!(got[..40].iter().all(|&v| v == 0.0), "channel {idx}");
        check!(got[40..44] == ramp(4, 40)[..], "channel {idx}");
    }
}
