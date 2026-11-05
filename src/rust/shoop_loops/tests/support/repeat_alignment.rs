use super::*;

#[shoop_test]
fn real_audio_and_midi_finish_their_pass_then_wait_and_resume_under_the_shared_policy() {
    for composite in [false, true] {
        for parts in [vec![29], vec![1; 29], vec![7, 8, 14]] {
            let mut h = Harness::new(true, 1024, false, staging());
            let target = id(if composite { 2 } else { 1 });
            h.op(
                0,
                Operation::Transition {
                    target: id(0),
                    timing: Timing::At,
                    transition: Transition::Record {
                        monitor_previous: false,
                    },
                },
            );
            h.op(10, play(0));
            let policy = h.op(
                10,
                Operation::RepeatSync {
                    target,
                    enabled: true,
                },
            );
            h.op(10, if composite { play(2) } else { play(1) });
            let waiting = h.op(15, Operation::State { target });
            let resumed = h.op(20, Operation::State { target });
            h.op(22, stop(0));
            h.op(28, if composite { stop(2) } else { stop(1) });
            for frames in parts {
                h.step(frames);
            }
            h.drain();
            assert_eq!(h.reply(policy), Reply::Done);
            let Reply::State(wait) = h.reply(waiting) else {
                panic!("wait state")
            };
            assert_eq!(wait.state.mode, Mode::Playing);
            assert_eq!(wait.state.position, 4);
            assert!(wait.state.repeat_sync);
            if composite {
                assert!(wait.state.composite.unwrap().waiting_repeat);
                assert_eq!(wait.state.composite.unwrap().passes, 1);
            }
            let Reply::State(resume) = h.reply(resumed) else {
                panic!("resume state")
            };
            assert_eq!(resume.state.mode, Mode::Playing);
            assert_eq!(resume.state.position, 0);
            assert_eq!(resume.state.last_pulse, Some(Frame(20)));
            let expected: Vec<_> = (0..29)
                .map(|frame| {
                    let sample = if (10..14).contains(&frame) {
                        (frame - 9) as f32 * 2.
                    } else if (20..28).contains(&frame) {
                        ((frame - 20) % 4 + 1) as f32 * 2.
                    } else {
                        0.
                    };
                    [sample, sample * 10.]
                })
                .collect();
            assert_eq!(h.samples, expected);
            let mut midi = vec![];
            for frame in [10, 20, 24] {
                restore(frame, &mut midi);
                midi.push((frame + 2, vec![0x80, 60, 45]));
            }
            assert_eq!(h.packets, midi);
            h.finish().unwrap();
        }
    }
}
