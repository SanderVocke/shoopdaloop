#![allow(dead_code)]
use rt_fpga_compiler::*;

pub fn mono_loops(quantum: u32, count: u32) -> LogicalStateProgram {
    let id = |slot| Identity {
        slot,
        generation: 1,
    };
    let mut initial_state = vec![
        InitialValue::Ramp(Ramp {
            start: 1 << 24,
            target: 1 << 24,
            duration: 0,
            elapsed: 0
        });
        count as usize
    ];
    let mut work_pass = vec![Operation::Input {
        port: 0,
        destination: 0,
    }];
    let mut bindings = Vec::new();
    for i in 0..count {
        let view = count + i;
        let playback = 1 + 2 * i;
        let output = 2 + 2 * i;
        initial_state.push(InitialValue::MediaView(i));
        work_pass.extend([
            Operation::Playback {
                loop_id: id(i),
                view,
                destination: playback,
            },
            Operation::Gain {
                ramp: i,
                source: playback,
                destination: output,
            },
            Operation::Output {
                source: output,
                port: i,
            },
            Operation::Capture {
                loop_id: id(i),
                view,
                source: 0,
            },
        ]);
        bindings.push(LoopBinding {
            id: id(i),
            initial_gain_bits: None,
            gain: Some(i),
            view: Some(view),
            meter: Some(output),
        });
    }
    LogicalStateProgram {
        composites: Vec::new(),
        quantum,
        chunk_frames: 1,
        sync_pass: (0..count).map(id).collect(),
        work_pass,
        initial_state,
        state_bindings: (0..count)
            .map(|channel| StateBinding {
                key: StateKey {
                    owner: shoop_graph::Owner::Loop(id(channel)),
                    role: StateRole::Gain,
                    channel: 0,
                },
                address: channel,
            })
            .chain((0..count).map(|channel| StateBinding {
                key: StateKey {
                    owner: shoop_graph::Owner::Loop(id(channel)),
                    role: StateRole::View,
                    channel: 0,
                },
                address: count + channel,
            }))
            .collect(),
        loops: (0..count)
            .map(|i| Loop {
                id: id(i),
                length: 0,
                sync_source: (i > 0).then(|| id(0)),
                repeat_sync: false,
            })
            .collect(),
        bindings,
        jobs_per_quantum: 1 + 5 * count,
        resources: Resources {
            queue_capacity: 1 + 5 * count,
            buffers: 1 + 2 * count,
            history_slots: 1 + 2 * count,
            input_ports: 1,
            output_ports: count,
            events: 4 * count,
        },
    }
}

pub fn sync_smoothing(quantum: u32) -> LogicalStateProgram {
    let id = Identity {
        slot: 0,
        generation: 7,
    };
    LogicalStateProgram {
        composites: Vec::new(),
        quantum,
        chunk_frames: 1,
        sync_pass: vec![id],
        work_pass: vec![
            Operation::Input {
                port: 0,
                destination: 0,
            },
            Operation::Gain {
                ramp: 1,
                source: 0,
                destination: 1,
            },
            Operation::Smoothing {
                loop_id: id,
                control: 0,
                source: 1,
            },
            Operation::Output { source: 2, port: 0 },
        ],
        state_bindings: vec![
            StateBinding {
                key: StateKey {
                    owner: shoop_graph::Owner::Loop(id),
                    role: StateRole::Smoothing(33),
                    channel: 0,
                },
                address: 0,
            },
            StateBinding {
                key: StateKey {
                    owner: shoop_graph::Owner::Loop(id),
                    role: StateRole::Gain,
                    channel: 0,
                },
                address: 1,
            },
        ],
        initial_state: vec![
            InitialValue::Smoothing {
                destination: 2,
                history: 7,
                duration: 33,
            },
            InitialValue::Ramp(Ramp {
                start: i32::MAX,
                target: i32::MIN,
                duration: 47,
                elapsed: 0,
            }),
        ],
        loops: vec![Loop {
            id,
            length: 4 * u64::from(quantum),
            sync_source: None,
            repeat_sync: false,
        }],
        bindings: vec![LoopBinding {
            id,
            initial_gain_bits: None,
            gain: Some(1),
            view: None,
            meter: Some(2),
        }],
        jobs_per_quantum: 8,
        resources: Resources {
            queue_capacity: 32,
            buffers: 8,
            history_slots: 16,
            input_ports: 1,
            output_ports: 1,
            events: 6,
        },
    }
}
