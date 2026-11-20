#[cfg(feature = "sim")]
fn main() {
    use rt_fpga_compiler::{
        CompileContext, MidiOperation as M, StateProgramTarget, compile_with_chunk_frames,
        shoop_graph::*,
    };
    use rt_fpga_testbench::target::RtlTarget;
    let mut graph = Graph::default();
    let id = |slot| Id {
        slot,
        generation: 1,
    };
    let port = |owner, role, channel| PortId {
        owner,
        signal: SignalType::Audio,
        role,
        index: channel,
    };
    for channel in 0..2 {
        for role in [PortRole::Input, PortRole::Output] {
            graph.ports.push(PortView {
                id: port(Owner::Host, role, channel),
                channel,
                name: "audio".into(),
            });
        }
    }
    for slot in 0..64 {
        graph.loops.push(LoopView {
            id: id(slot),
            strip: None,
            layout: ChannelLayout::Direct {
                audio: 2,
                midi: false,
            },
            gain: 1.0,
            balance: 0.,
            boundary_smoothing_frames: 0,
            sync_source: None,
            repeat_sync: false,
            composite: None,
        });
        for channel in 0..2 {
            let owner = Owner::Loop(id(slot));
            for role in [PortRole::Input, PortRole::Output] {
                graph.ports.push(PortView {
                    id: port(owner, role, channel),
                    channel,
                    name: "audio".into(),
                });
            }
            graph.connections.push(Connection {
                source: port(Owner::Host, PortRole::Input, channel),
                destination: port(owner, PortRole::Input, channel),
            });
            graph.connections.push(Connection {
                source: port(owner, PortRole::Output, channel),
                destination: port(Owner::Host, PortRole::Output, channel),
            });
        }
    }
    let logical = compile_with_chunk_frames(
        &graph,
        CompileContext {
            quantum: 32,
            limits: Default::default(),
        },
        32,
    )
    .unwrap();
    let mut operations = vec![M::Input {
        port: 0,
        destination: 0,
    }];
    for slot in 0..64 {
        operations.push(M::Capture {
            loop_id: id(slot),
            sequence: slot as u16,
            source: 0,
        });
    }
    for slot in 0..64 {
        operations.push(M::Playback {
            loop_id: id(slot),
            sequence: slot as u16,
            cursor: slot as u16,
            channels: 1 << (slot % 16),
            destination: slot as u16 + 1,
        });
    }
    let mut merged = 1;
    for slot in 1..64 {
        let destination = 64 + slot;
        operations.push(M::Merge {
            left: merged,
            right: slot + 1,
            destination,
        });
        merged = destination;
    }
    operations.push(M::Output {
        source: merged,
        port: 0,
    });
    let logical = logical.with_midi_data(&operations).unwrap();
    let views = (0..128).collect::<Vec<_>>();
    let cap = [2048, 256, 2048, 32, 512, 2, 2];
    let image = RtlTarget::new(cap, 256, &views).build(&logical).unwrap();
    println!("{}",serde_json::to_string_pretty(&serde_json::json!({
        "loops":64,"audio_channels_per_loop":2,"midi_channels_per_loop":1,
        "sample_rate":48000,"seconds":30,"chunk_frames":32,"quantum_frames":32,
        "compiled_instructions":image.instructions.len(),"compiled_sync_instructions":image.sync_count,
        "state_values":image.state.len(),"jobs_per_quantum":image.jobs_per_quantum,
        "audio_buffers":image.resources.buffers,"audio_inputs":image.resources.input_ports,
        "audio_outputs":image.resources.output_ports,"midi_sequences":64,"midi_cursors":64,"midi_buffers":128,"midi_scratch_events":131072,
        "audio_read_operations":logical.work_pass.iter().filter(|op|matches!(op,rt_fpga_compiler::Operation::Playback {..})).count(),
        "read_hint_entries":256,
        "capacity":{"instructions":2048,"state_values":256,"jobs":2048,"buffers":512,"loop_slots":256},
        "program_words":image.instructions.iter().map(|b|b.array::<3>()).collect::<Vec<_>>()
    })).unwrap());
}
#[cfg(not(feature = "sim"))]
fn main() {
    panic!("Enable sim to prepare the MIDI synthesis profile");
}
