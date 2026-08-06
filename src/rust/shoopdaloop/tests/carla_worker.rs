use assert_no_alloc::assert_no_alloc;
use shoop_engine::carla_processor::{CarlaProcessor, CarlaProcessorLifecycle};
use shoop_engine::carla_subprocess::{SubprocessCarlaProcessor, SupervisedCarlaProcessor};
use shoop_engine::FXChainType;
use shoop_plugin_protocol::{ChainId, ProcessGeneration};

#[test]
fn self_spawned_carla_worker_processes_and_preserves_state() {
    let executable = env!("CARGO_BIN_EXE_shoopdaloop");
    let mut processor = match SubprocessCarlaProcessor::spawn(
        executable,
        FXChainType::CarlaRack,
        48_000,
        256,
        ChainId(1),
        ProcessGeneration(1),
    ) {
        Ok(processor) => processor,
        Err(error) if error.to_string().contains("not found in LV2_PATH") => {
            eprintln!("skipping Carla worker test: {error}");
            return;
        }
        Err(error) => panic!("could not start Carla worker: {error:#}"),
    };

    processor.set_active(true);
    for (index, sample) in processor.audio_input_mut(0).unwrap()[..256]
        .iter_mut()
        .enumerate()
    {
        *sample = (index as f32 * 0.01).sin();
    }
    processor
        .set_midi_input_events(0, &[(7, &[0x90, 64, 100]), (31, &[0x80, 64, 0])])
        .unwrap();
    processor.process(256).expect("subprocess block");
    assert_no_alloc(|| {
        processor
            .process(256)
            .expect("allocation-free subprocess block");
    });
    assert!(processor.audio_output(0).unwrap()[..256]
        .iter()
        .all(|sample| sample.is_finite()));

    let state = processor.save_state().expect("worker state save");
    assert!(state.starts_with('{'));
    processor
        .restore_state(&state)
        .expect("worker state restore");
    let status = processor.status().expect("worker status");
    assert!(status.ready);
    assert!(status.active);
    assert_eq!(status.processed_blocks, 2);
}

#[test]
fn supervisor_detects_crash_preserves_checkpoint_and_starts_new_generation() {
    let executable = env!("CARGO_BIN_EXE_shoopdaloop");
    let mut supervisor = SupervisedCarlaProcessor::launch(
        executable,
        FXChainType::CarlaRack,
        48_000,
        256,
        ChainId(2),
    )
    .expect("construct supervisor");
    if supervisor.lifecycle() == CarlaProcessorLifecycle::Unavailable {
        eprintln!(
            "skipping Carla supervisor test: {}",
            supervisor.crash_summary().unwrap_or("worker unavailable")
        );
        return;
    }

    supervisor.set_active(true);
    let checkpoint = supervisor.save_state().expect("initial checkpoint");
    supervisor
        .terminate_worker_for_test()
        .expect("terminate worker");
    assert!(!supervisor.is_ready());
    assert_eq!(supervisor.lifecycle(), CarlaProcessorLifecycle::Crashed);
    assert_eq!(supervisor.generation(), 1);
    assert_eq!(
        supervisor.save_state().expect("fallback checkpoint"),
        checkpoint
    );

    supervisor
        .restart_without_ui_for_test()
        .expect("restart worker");
    assert!(supervisor.is_ready());
    assert!(supervisor.is_active());
    assert_eq!(supervisor.generation(), 2);
    assert_eq!(supervisor.save_state().unwrap(), checkpoint);
    let generations: Vec<_> = supervisor
        .generation_logs()
        .into_iter()
        .map(|log| log.generation)
        .collect();
    assert_eq!(generations, vec![1, 2]);
}

#[test]
fn separate_chains_use_independent_worker_processes() {
    let executable = env!("CARGO_BIN_EXE_shoopdaloop");
    let mut first = SupervisedCarlaProcessor::launch(
        executable,
        FXChainType::CarlaRack,
        48_000,
        256,
        ChainId(11),
    )
    .unwrap();
    let mut second = SupervisedCarlaProcessor::launch(
        executable,
        FXChainType::CarlaRack,
        48_000,
        256,
        ChainId(12),
    )
    .unwrap();
    if !first.is_ready() || !second.is_ready() {
        eprintln!("skipping independent worker test because Carla is unavailable");
        return;
    }
    assert_ne!(first.worker_id(), second.worker_id());
    first.set_active(true);
    second.set_active(true);
    first.terminate_worker_for_test().unwrap();
    assert!(!first.is_ready());
    assert!(second.is_ready());
    second.process(256).expect("unaffected worker block");
    assert!(second.is_ready());
}
