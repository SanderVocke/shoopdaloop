use shoop_engine::carla_processor::CarlaProcessor;
use shoop_engine::carla_subprocess::SubprocessCarlaProcessor;
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
    assert_eq!(status.processed_blocks, 1);
}
