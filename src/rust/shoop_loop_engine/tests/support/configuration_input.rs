use super::*;

fn check_initial(
    state: rt_runtime::midi_state::ChannelState,
    channel: u8,
    preserve: bool,
    second: bool,
) {
    assert_eq!(
        (
            state.program(),
            state.program_bank(),
            state.pitch_bend(),
            state.pressure(),
            state.reset_controllers_seen(),
            state.parameter_family(),
            state.parameter_history_untracked()
        ),
        (None, [None, None], 8192, None, false, None, false)
    );
    for number in 0..128 {
        let controller = match number {
            1 if channel == 2 && preserve => Some(37),
            64 if channel == 2 && preserve => Some(127),
            64 | 66 | 69 => Some(0),
            _ => None,
        };
        assert_eq!(
            state.controller(number),
            controller,
            "channel {channel} controller {number}"
        );
        let (velocity, held) = match number {
            61 if channel == 2 && preserve => (91, false),
            62 if channel == 2 && preserve => (72, true),
            63 if channel == 2 && second => (55, true),
            _ => (0, false),
        };
        let note = state.note(number).unwrap();
        assert_eq!(
            (
                note.velocity(),
                note.held(),
                note.sostenuto(),
                note.pressure()
            ),
            (velocity, held, false, None),
            "channel {channel} note {number}"
        );
    }
}
#[shoop_test]
fn input_projection_is_explicitly_preserved_or_reset_once_before_the_first_new_capture() {
    for preserve in [false, true] {
        for partitioned in [false, true] {
            let mut h = prepared(true, 0, false, 64, false);
            let sources = source_pair(&mut h);
            let roles = begin(&mut h, 0, 1);
            let target = generation(1, 2);
            h.op(
                3,
                Operation::RoleAppend {
                    id: roles,
                    previous: Some(id(1)),
                    initial: Some(initial(target, sources, preserve)),
                },
            );
            h.step(1);
            h.drain();
            let topology = h.ready(
                0,
                &compile(limits(0), &changed_description(2, false)).unwrap(),
            );
            assert_no_heap(|| h.control.parameter(Frame(6), node(4), u32::MAX, 1.))
                .unwrap_or_else(|e| panic!("{:?}", e.error));
            let graph = h.graph(0, 1, false, false);
            let activated = h.op(7, configuration(roles, topology, graph, 1, 0));
            h.op(
                7,
                transition(
                    target,
                    Transition::Record {
                        monitor_previous: false,
                    },
                ),
            );
            assert_no_heap(|| h.control.parameter(Frame(7), node(4), u32::MAX, 2.))
                .unwrap_or_else(|e| panic!("{:?}", e.error));
            let prefix = h.op(
                8,
                Operation::Export {
                    target,
                    selection: Selection::Recording,
                },
            );
            h.op(9, transition(target, Transition::Stop));
            let full = h.op(
                9,
                Operation::Export {
                    target,
                    selection: Selection::Latest,
                },
            );
            h.op(
                10,
                transition(
                    target,
                    Transition::Record {
                        monitor_previous: false,
                    },
                ),
            );
            let second = h.op(
                11,
                Operation::Export {
                    target,
                    selection: Selection::Recording,
                },
            );
            h.op(12, transition(target, Transition::Stop));
            if partitioned {
                for _ in 0..6 {
                    h.step(1);
                }
            } else {
                h.step(6);
            }
            h.drain();
            assert_eq!(
                h.reply(activated),
                Reply::ConfigurationActivated {
                    roles,
                    topology,
                    graph,
                    graph_epoch: Epoch(2),
                    topology_epoch: Epoch(1)
                }
            );
            let expected: Vec<_> = [
                [0xb2, 1, 37],
                [0x92, 61, 91],
                [0xb2, 64, 127],
                [0x82, 61, 0],
                [0x92, 62, 72],
            ]
            .into_iter()
            .map(|p| (6, p.to_vec()))
            .chain(std::iter::once((7, vec![0x92, 63, 55])))
            .collect();
            assert_eq!(h.packets, expected);
            for (sequence, frames, is_second) in
                [(prefix, 1, false), (full, 2, false), (second, 1, true)]
            {
                let Reply::Export {
                    id: export,
                    cut,
                    media,
                } = h.reply(sequence)
                else {
                    panic!("export")
                };
                assert_eq!(
                    (
                        cut.graph_epoch,
                        cut.topology_epoch,
                        media.view.description.owner,
                        media.view.description.frames,
                        media.view.description.revision
                    ),
                    (
                        Epoch(2),
                        Epoch(1),
                        target,
                        frames,
                        Revision(if is_second { 2 } else { 1 })
                    )
                );
                for channel in 0..16 {
                    let sequence = h.op(
                        h.engine.next_frame().0,
                        Operation::MidiInitial { export, channel },
                    );
                    h.step(1);
                    h.drain();
                    let Reply::MidiInitial {
                        channel: actual,
                        state,
                        system_reset_seen,
                    } = h.reply(sequence)
                    else {
                        panic!("initial")
                    };
                    assert_eq!(actual, channel);
                    assert!(!system_reset_seen);
                    check_initial(state, channel, preserve, is_second);
                }
                let packets = if is_second {
                    vec![]
                } else {
                    encoded_packets(&[(0, vec![0x92, 63, 55])])
                };
                assert_eq!(h.read_export(export, true, packets.len()), packets);
                assert_eq!(
                    h.read_export(export, false, frames as usize * 8),
                    vec![0; frames as usize * 8]
                );
            }
            assert!(h.samples.iter().all(|s| *s == [0., 0.]));
            assert_eq!(h.packets, expected);
            h.finish();
        }
    }
}
