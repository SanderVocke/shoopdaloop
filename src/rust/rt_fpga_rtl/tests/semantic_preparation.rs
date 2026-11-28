#![cfg(feature = "rt-interface")]
#[path = "../../rt_fpga_compiler/tests/support/mono.rs"]
mod mono;
use rt_fpga_compiler::{
    CompileContext, CompileError, Identity, LogicalStateProgram, ProgramLimits,
    compile_with_chunk_frames, loading::Allocator,
};
use rt_fpga_rtl::{
    host::{
        UploadStep,
        semantic::{CommandRecord, Preparation, PreparationStep},
    },
    target::{ProgramImage, RtlTarget, Space},
};
use shoop_wasm_test_support::shoop_test;

fn limits() -> ProgramLimits {
    ProgramLimits {
        instructions: 1024,
        state_values: 512,
        loop_slots: 128,
        jobs: 512,
        buffers: 512,
        history_slots: 512,
        input_ports: 1,
        output_ports: 2,
        quantum: 32,
    }
}
fn id(slot: u32) -> Identity {
    Identity {
        slot,
        generation: 7,
    }
}
fn logical(entries: &[(u32, f32, Option<u32>)]) -> LogicalStateProgram {
    let loops: Vec<_> = entries
        .iter()
        .map(|&(slot, gain, source)| mono::MonoLoop {
            id: id(slot),
            source,
            input: true,
            output: true,
            gain,
            repeat_sync: false,
        })
        .collect();
    compile_with_chunk_frames(
        &mono::session(&loops),
        CompileContext {
            quantum: 32,
            limits: limits(),
        },
        32,
    )
    .unwrap()
}
fn image(allocator: &mut Allocator, program: &LogicalStateProgram, semantic: bool) -> ProgramImage {
    let layout = allocator.stage(program).unwrap();
    let views = layout.media_views(program);
    let target = RtlTarget {
        capacities: limits(),
        media_views: &views,
        state_base: 0,
    };
    if semantic {
        target.build_semantic_image(program, layout)
    } else {
        target.build_image(program, layout)
    }
    .unwrap()
}

#[shoop_test]
fn original_gain_bits_survive_compilation_and_inconsistent_recipes_refuse() {
    let program = logical(&[(11, 0.12345679, None), (27, -0.0, Some(11))]);
    assert_eq!(
        program.bindings[0].initial_gain_bits,
        Some(0.12345679_f32.to_bits())
    );
    assert_eq!(
        program.bindings[1].initial_gain_bits,
        Some((-0.0_f32).to_bits())
    );
    let compiled = image(&mut Allocator::new(limits()), &program, true);
    for (source, bound) in program.bindings.iter().zip(&compiled.compiled.bindings) {
        assert_eq!(source.initial_gain_bits, bound.initial_gain_bits);
    }
    let address = compiled.compiled.bindings[0].gain.unwrap();
    let fixed = compiled.compiled.state[usize::from(address)].get(0, 32) as i32;
    assert_ne!(
        (fixed as f32 / (1 << 24) as f32).to_bits(),
        0.12345679_f32.to_bits()
    );
    for bits in [
        f32::NAN.to_bits(),
        f32::INFINITY.to_bits(),
        (-f32::INFINITY).to_bits(),
        0.5_f32.to_bits(),
    ] {
        let mut invalid = program.clone();
        invalid.bindings[0].initial_gain_bits = Some(bits);
        assert_eq!(
            invalid.validate_logical(limits()),
            Err(CompileError::InvalidProgram)
        );
    }
    let mut unknown = program;
    unknown.bindings[0].initial_gain_bits = None;
    unknown.validate_logical(limits()).unwrap();
    assert!(Preparation::new(image(&mut Allocator::new(limits()), &unknown, true)).is_err());
}

#[shoop_test]
fn semantic_lowering_moves_the_whole_sync_prefix_without_reordering_jobs() {
    let program = logical(&[(11, 0.12345679, None), (27, 0.75, Some(11))]);
    let legacy = image(&mut Allocator::new(limits()), &program, false);
    let semantic = image(&mut Allocator::new(limits()), &program, true);
    assert_eq!(legacy.compiled.instructions[0].get(0, 8), 0);
    assert!(
        semantic.compiled.instructions[..2]
            .iter()
            .all(|word| word.get(0, 8) == 12)
    );
    let jobs = |image: &ProgramImage| {
        image
            .compiled
            .instructions
            .iter()
            .filter(|word| word.get(0, 8) != 12)
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(jobs(&legacy), jobs(&semantic));
    assert!(Preparation::new(legacy).is_err());
    let preparation = Preparation::new(semantic).unwrap();
    assert_eq!(preparation.descriptor().sync_count, 2);
    assert_eq!(
        preparation.target(id(11)).unwrap(),
        rt_fpga_rtl::host::semantic::BoundTarget::new(0, 1, Some(0), true).unwrap()
    );
    assert!(
        preparation
            .target(Identity {
                slot: 11,
                generation: 8
            })
            .is_none()
    );
    assert!(preparation.target(id(999)).is_none());
    let command = CommandRecord::gain(preparation.target(id(27)), 0.33333334);
    assert_eq!(command.words()[3], 0);
    assert_eq!(command.words()[5], 0.33333334_f32.to_bits());
}

#[shoop_test]
fn fresh_observations_follow_preparation_and_never_initialize_continuing_allocations() {
    let mut allocator = Allocator::new(limits());
    let first = logical(&[(11, 0.12345679, None), (27, -0.0, Some(11))]);
    let first_image = image(&mut allocator, &first, true);
    let retained = first_image.compiled.bindings[0].gain.unwrap();
    let old = first_image.layout.loop_id(id(11)).unwrap();
    let mut preparation = Preparation::new(first_image).unwrap();
    let mut observed = Vec::new();
    let mut metadata_started = false;
    while let Some(step) = preparation.next_step() {
        match step {
            PreparationStep::Observation { slot, word, value } => {
                metadata_started = true;
                observed.push((slot, word, value));
            }
            PreparationStep::Upload(_) => assert!(!metadata_started),
        }
    }
    assert_eq!(
        observed,
        [
            (0, 0, u64::from(0.12345679_f32.to_bits())),
            (0, 1, 0),
            (0, 2, 0),
            (1, 0, u64::from((-0.0_f32).to_bits())),
            (1, 1, 0),
            (1, 2, 0),
        ]
    );
    assert!(preparation.next_step().is_none());
    // Exercise allocator bookkeeping only; this is not a hardware ownership fence.
    allocator.publish(1).unwrap();
    allocator.complete(1).unwrap();
    let next = logical(&[(11, 0.91, Some(99)), (99, 0.375, None)]);
    let mut preparation = Preparation::new(image(&mut allocator, &next, true)).unwrap();
    assert_eq!(preparation.image().layout.loop_id(id(11)).unwrap(), old);
    assert_eq!(preparation.image().new_loops[0].id.slot, 2);
    assert_eq!(
        preparation.image().compiled.bindings[0].gain.unwrap(),
        retained
    );
    let command = CommandRecord::gain(preparation.target(id(11)), -0.0);
    assert_eq!(command.words()[1], old.slot | (u32::from(retained) << 16));
    assert_eq!(command.words()[3], 0);
    observed.clear();
    while let Some(step) = preparation.next_step() {
        match step {
            PreparationStep::Observation { slot, word, value } => {
                observed.push((slot, word, value))
            }
            PreparationStep::Upload(UploadStep::Configure(write)) => match write.space {
                Space::State => assert_ne!(write.address, u64::from(retained)),
                Space::Loop => assert_ne!(write.address / 16, u64::from(old.slot)),
                Space::Program => assert!(write.address >= 1024),
                Space::History => {}
            },
            PreparationStep::Upload(UploadStep::ClearView(_)) => {}
        }
    }
    assert_eq!(
        observed,
        [(2, 0, u64::from(0.375_f32.to_bits())), (2, 1, 0), (2, 2, 0)]
    );
}

#[shoop_test]
fn allocation_witness_survives_lowering_without_making_preparation_a_lease() {
    let (mut allocator, verifier) = Allocator::with_verifier(limits());
    let first = logical(&[(11, 0.12345679, None)]);
    let preparation = Preparation::new(image(&mut allocator, &first, true)).unwrap();
    let active = &preparation.image().layout;
    assert_eq!(verifier.verify_transition(None, active), Ok(()));
    // Allocator bookkeeping only; the device owner must separately match an actual fence.
    allocator.publish(active.revision).unwrap();
    allocator.complete(active.revision).unwrap();
    let next = logical(&[(11, 0.75, None), (27, -0.0, Some(11))]);
    let next = Preparation::new(image(&mut allocator, &next, true)).unwrap();
    assert_eq!(
        verifier.verify_transition(Some(active), &next.image().layout),
        Ok(())
    );
    assert!(
        verifier
            .verify_transition(None, &next.image().layout)
            .is_err()
    );
    let mut tampered = next.image().layout.clone();
    tampered.loops[0].fresh = true;
    assert!(verifier.verify_transition(Some(active), &tampered).is_err());
    let (_, other_domain) = Allocator::with_verifier(limits());
    assert!(
        other_domain
            .verify_transition(Some(active), &next.image().layout)
            .is_err()
    );
}

#[shoop_test]
fn malformed_images_cannot_become_semantic_preparation_recipes() {
    let program = logical(&[(11, 0.25, None)]);
    for change in 0..13 {
        let mut candidate = image(&mut Allocator::new(limits()), &program, true);
        match change {
            0 => candidate.compiled.quantum = 16,
            1 => candidate.compiled.chunk_frames = 16,
            2 => candidate.layout.region = 2,
            3 => candidate.layout.revision = 0,
            4 => candidate.compiled.resources.queue_capacity = 513,
            5 => candidate.compiled.resources.input_ports = 2,
            6 => candidate.compiled.bindings[0].gain = Some(512),
            7 => candidate.compiled.bindings[0].initial_gain_bits = Some(f32::NAN.to_bits()),
            8 => candidate.initializers[0].0 = 511,
            9 => candidate.new_loops.clear(),
            10 => {
                candidate.compiled.instructions[0].set(0, 8, 0);
            }
            11 => candidate
                .compiled
                .instructions
                .push(candidate.compiled.instructions[0].clone()),
            _ => candidate.compiled.sync_count = 2,
        };
        assert!(Preparation::new(candidate).is_err(), "change {change}");
    }
}
