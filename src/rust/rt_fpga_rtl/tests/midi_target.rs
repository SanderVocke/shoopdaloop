#[path = "../../rt_fpga_compiler/tests/support/programs.rs"]
mod programs;
use rt_fpga_compiler::{CompileError, MidiBinding, MidiOperation as M, StateProgramTarget};
use rt_fpga_rtl::{
    bits::Bits,
    component::{self, Identity, Instruction},
    target::RtlTarget,
};
use shoop_wasm_test_support::shoop_test;
#[shoop_test]
fn midi_playback_instruction_keeps_full_loop_generation_and_all_channel_bits() {
    let words = component::instruction_words(Instruction::MidiPlayback {
        loop_id: Identity {
            slot: 0x1234,
            generation: 0xfedcba98,
        },
        sequence: 255,
        cursor: 254,
        channels: 0x8181,
        destination: 253,
    })
    .unwrap();
    let bits = Bits::from(words);
    for (offset, width, expected) in [
        (0, 8, 15),
        (8, 16, 0x1234),
        (24, 32, 0xfedcba98),
        (56, 8, 255),
        (64, 8, 254),
        (72, 16, 0x8181),
        (88, 8, 253),
    ] {
        assert_eq!(bits.get(offset, width), expected);
    }
}
#[shoop_test]
fn midi_target_lowers_data_jobs_and_rejects_legacy_phases_and_wide_read_operands() {
    let mut base = programs::mono_loops(32, 2);
    base.work_pass.clear();
    base.jobs_per_quantum = 0;
    base.resources.buffers = 512;
    let target = RtlTarget::new([64, 16, 64, 128, 512, 2, 2], 16, &[0, 1]);
    let id = base.loops[0].id;
    let operations = [
        M::Input {
            port: 0,
            destination: 0,
        },
        M::Capture {
            loop_id: id,
            sequence: 0,
            source: 0,
        },
        M::Playback {
            loop_id: id,
            sequence: 0,
            cursor: 0,
            channels: u16::MAX,
            destination: 1,
        },
        M::Output { source: 1, port: 0 },
    ];
    let valid = base.clone().with_midi_data(&operations).unwrap();
    let image = target.build(&valid).unwrap();
    assert_eq!(
        image
            .instructions
            .iter()
            .map(|b| b.get(0, 8))
            .collect::<Vec<_>>(),
        vec![12, 12, 14, 16, 15, 18]
    );
    for which in 0..3 {
        let mut ops = operations;
        ops[2] = match which {
            0 => M::Playback {
                loop_id: id,
                sequence: 256,
                cursor: 0,
                channels: 1,
                destination: 1,
            },
            1 => M::Playback {
                loop_id: id,
                sequence: 0,
                cursor: 256,
                channels: 1,
                destination: 1,
            },
            _ => M::Playback {
                loop_id: id,
                sequence: 0,
                cursor: 0,
                channels: 1,
                destination: 256,
            },
        };
        let invalid = base.clone().with_midi_data(&ops).unwrap();
        invalid.validate(target.limits()).unwrap();
        assert!(matches!(
            target.build(&invalid),
            Err(CompileError::InvalidProgram)
        ));
    }
    let legacy = base
        .with_midi(&[MidiBinding {
            loop_id: id,
            context: 0,
        }])
        .unwrap();
    assert!(matches!(
        target.build(&legacy),
        Err(CompileError::InvalidProgram)
    ));
}
