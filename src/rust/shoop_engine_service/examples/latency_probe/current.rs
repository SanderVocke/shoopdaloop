use super::scenario::{dry, Evidence, Partition};
use shoop_engine::{audio_channel::AudioChannel, channel_mode::ChannelMode, loop_mode::LoopMode};
use shoop_latency::{
    wet_recording_offset, ProcessorRenderAdvance, RecordingOffset, RetentionWindow,
};
use shoop_wasm_test_support::heap;

fn block(partition: Partition, remaining: usize, pattern: &mut usize) -> usize {
    match partition {
        Partition::Whole => remaining,
        Partition::Single => 1,
        Partition::Irregular => {
            let n = [3, 1, 2][*pattern % 3];
            *pattern += 1;
            n.min(remaining)
        }
    }
}
/// Compare the public current-engine channel's storage/alignment contract, not
/// its session scheduler, devices, MIDI or plugin graph.
pub fn verify(physical: &Evidence, print: bool) {
    for offset in [0, -2] {
        let dry_offset = RecordingOffset::new(offset).unwrap();
        let wet_offset =
            wet_recording_offset(dry_offset, ProcessorRenderAdvance::new(3).unwrap()).unwrap();
        for partition in [Partition::Whole, Partition::Single, Partition::Irregular] {
            for (channel, alignment) in [dry_offset, wet_offset].into_iter().enumerate() {
                let retention = RetentionWindow::for_offset(alignment);
                let before = retention.before_frames() as usize;
                let after = retention.after_frames() as usize;
                let mut old = AudioChannel::with_bounded_capacity(4, 16, ChannelMode::Direct);
                old.prepare_latency_retention(8, before as u32, after as u32)
                    .unwrap();
                old.set_capture_alignment_frames(alignment.frames())
                    .unwrap();
                old.set_loop_smoothing_frames(0);
                let start = 4 - before;
                let mut pattern = 0;
                for (begin, end, mode, prerecord) in [
                    (start, 4, LoopMode::Stopped, true),
                    (4, 12, LoopMode::Recording, false),
                    (12, 16, LoopMode::Stopped, false),
                ] {
                    let mut at = begin;
                    while at < end {
                        let n = block(partition, end - at, &mut pattern);
                        heap::assert_no_heap(|| {
                            let mut input = [0.; 16];
                            let mut output = [0.; 16];
                            for (i, value) in input[..n].iter_mut().enumerate() {
                                *value = if channel == 0 {
                                    dry(physical.signal, (at + i) as u64)
                                } else {
                                    physical.physical_wet[at + i]
                                };
                            }
                            old.set_recording_buffer_size(n);
                            old.set_playback_buffer_size(n);
                            old.process(
                                mode,
                                if prerecord {
                                    LoopMode::Recording
                                } else {
                                    LoopMode::Unknown
                                },
                                prerecord.then_some(0),
                                prerecord.then_some((4 - at.min(4)) as u32),
                                n,
                                0,
                                if mode == LoopMode::Recording {
                                    at - 4
                                } else {
                                    8
                                },
                            )
                            .unwrap();
                            old.finalize_process(&input[..n], &mut output[..n]);
                            assert_eq!(&output[..n], &[0.; 16][..n]);
                        });
                        at += n;
                    }
                }
                assert!(!old.is_finalizing_latency_postroll());
                assert!(!old.latency_retention_incomplete());
                assert_eq!(old.media_layout_offset(), before as i32);
                assert_eq!(old.length(), 8 + before + after);
                let raw = old.data();
                let expected_raw: Vec<f32> = (start..12 + after)
                    .map(|at| {
                        if channel == 0 {
                            dry(physical.signal, at as u64)
                        } else {
                            physical.physical_wet[at]
                        }
                    })
                    .collect();
                assert_eq!(raw, expected_raw);
                assert_eq!(
                    old.raw_position_for_logical(0),
                    Some(before as i32 + alignment.frames())
                );
                let mut playback = Vec::with_capacity(16);
                for _ in 0..2 {
                    let mut pos = 0;
                    while pos < 8 {
                        let n = block(partition, 8 - pos, &mut pattern);
                        let mut output = [0.; 16];
                        heap::assert_no_heap(|| {
                            old.set_recording_buffer_size(n);
                            old.set_playback_buffer_size(n);
                            old.process(
                                LoopMode::Playing,
                                LoopMode::Unknown,
                                None,
                                None,
                                n,
                                pos as i32,
                                8,
                            )
                            .unwrap();
                            old.finalize_process(&[0.; 16][..n], &mut output[..n]);
                        });
                        playback.extend_from_slice(&output[..n]);
                        pos += n;
                    }
                }
                let expected: Vec<f32> = (0..16)
                    .map(|i| dry(physical.signal, (4 + offset + i % 8) as u64))
                    .collect();
                assert_eq!(playback, expected);
                if print && matches!(partition, Partition::Whole) {
                    println!("current_engine,offset={offset},channel={channel},physical_start={start},raw={raw:?},playback={playback:?}");
                }
            }
        }
    }
}
