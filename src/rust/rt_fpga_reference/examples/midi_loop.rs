use rt_fpga_compiler::{
    Identity, LogicalStateProgram, MidiOperation as M, Resources, StateProgramTarget,
};
use rt_fpga_reference::{
    jobs::JobProcessor,
    midi::{BufferLimits, InputEvent, Message, MidiMemory, StoreLimits},
    pipeline::Pipeline,
    state::ModePlan,
    storage::MediaStore,
    target::RustTarget,
    transitions::{LoopId, Mode},
};

fn main() {
    let logical_id = Identity {
        slot: 0,
        generation: 1,
    };
    let logical = LogicalStateProgram {
        quantum: 32,
        chunk_frames: 32,
        sync_pass: vec![logical_id],
        work_pass: vec![],
        initial_state: vec![],
        state_bindings: vec![],
        loops: vec![rt_fpga_compiler::Loop {
            id: logical_id,
            length: 0,
            sync_source: None,
            repeat_sync: false,
        }],
        bindings: vec![rt_fpga_compiler::LoopBinding {
            id: logical_id,
            gain: None,
            view: None,
            meter: None,
        }],
        jobs_per_quantum: 0,
        resources: Resources {
            queue_capacity: 8,
            buffers: 2,
            history_slots: 0,
            input_ports: 1,
            output_ports: 1,
            events: 4,
        },
    }
    .with_midi_data(&[
        M::Input {
            port: 0,
            destination: 0,
        },
        M::Capture {
            loop_id: logical_id,
            sequence: 0,
            source: 0,
        },
        M::Playback {
            loop_id: logical_id,
            sequence: 0,
            cursor: 0,
            channels: u16::MAX,
            destination: 1,
        },
        M::Output { source: 1, port: 0 },
    ])
    .unwrap();
    let compiled = RustTarget::new(&[]).build(&logical).unwrap();
    let mut pipeline =
        Pipeline::prepare(compiled.plan, compiled.program, compiled.memory, 8).unwrap();
    let mut jobs = JobProcessor::new(32, 1);
    jobs.prepare_midi(BufferLimits {
        buffers: 2,
        cursors: 1,
        scratch_events: 8192,
        input_ports: 1,
        output_ports: 1,
    })
    .unwrap();
    let mut audio = MediaStore::new(32).unwrap();
    let mut midi = MidiMemory::prepare(
        StoreLimits {
            block_events: 256,
            blocks: 32,
            max_events: 8192,
            views: 8,
            retirements: 8,
        },
        1,
    )
    .unwrap();
    let input: Vec<_> = (0..3000)
        .map(|i| InputEvent {
            input: 0,
            offset: 0,
            message: Message::parse(&[0xb0, 1, (i % 128) as u8]).unwrap(),
        })
        .collect();
    let id = LoopId {
        slot: 0,
        generation: 1,
    };
    pipeline
        .write_mode_plan(
            id,
            ModePlan {
                now: Some(Mode::Recording),
                ..Default::default()
            },
        )
        .unwrap();
    pipeline
        .process_with_midi(
            &mut jobs,
            &mut audio,
            &[],
            &mut [],
            32,
            &mut midi,
            &input,
            &mut |_| Ok(()),
        )
        .unwrap();
    pipeline
        .write_mode_plan(
            id,
            ModePlan {
                now: Some(Mode::Playing),
                ..Default::default()
            },
        )
        .unwrap();
    let mut output = Vec::with_capacity(input.len());
    let work = pipeline
        .process_with_midi(
            &mut jobs,
            &mut audio,
            &[],
            &mut [],
            32,
            &mut midi,
            &[],
            &mut |event| {
                output.push(event);
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(output.len(), 3000);
    println!("Recorded and played {} messages in one 32-frame quantum across 12 storage blocks. Work: {work:?}",output.len());
}
