#![cfg(target_os = "linux")]

use shoop_engine::carla_processor::{spawn_processor_bridge, FakeCarlaProcessor};
use shoop_engine::FXChainType;

struct CallerState {
    affinity: libc::cpu_set_t,
    policy: i32,
    parameters: libc::sched_param,
}

impl CallerState {
    fn capture() -> Self {
        unsafe {
            let mut state = Self {
                affinity: std::mem::zeroed(),
                policy: libc::sched_getscheduler(0),
                parameters: std::mem::zeroed(),
            };
            assert_eq!(
                libc::sched_getaffinity(
                    0,
                    std::mem::size_of_val(&state.affinity),
                    &mut state.affinity
                ),
                0
            );
            assert_eq!(libc::sched_getparam(0, &mut state.parameters), 0);
            state
        }
    }

    fn restore_policy(&self) {
        assert_eq!(
            unsafe { libc::sched_setscheduler(0, self.policy, &self.parameters) },
            0
        );
    }
}

impl Drop for CallerState {
    fn drop(&mut self) {
        self.restore_policy();
        assert_eq!(
            unsafe {
                libc::sched_setaffinity(0, std::mem::size_of_val(&self.affinity), &self.affinity)
            },
            0
        );
    }
}

#[shoop_wasm_test_support::shoop_test]
fn realtime_caller_allows_an_ordinary_worker_to_run_on_the_same_cpu() {
    let original = CallerState::capture();
    assert_eq!(original.policy, libc::SCHED_OTHER);
    let priority = unsafe { libc::sched_get_priority_min(libc::SCHED_FIFO) } + 1;
    let parameters = libc::sched_param {
        sched_priority: priority,
    };
    if unsafe { libc::sched_setscheduler(0, libc::SCHED_FIFO, &parameters) } != 0 {
        let error = std::io::Error::last_os_error();
        assert_eq!(
            std::env::var("SHOOP_ALLOW_MISSING_BACKENDS").as_deref(),
            Ok("1"),
            "realtime scheduling is required: {error}"
        );
        eprintln!("explicitly skipping unavailable realtime scheduling: {error}");
        return;
    }
    original.restore_policy();
    let mut affinity: libc::cpu_set_t = unsafe { std::mem::zeroed() };
    let cpu = (0..libc::CPU_SETSIZE as usize)
        .find(|cpu| unsafe { libc::CPU_ISSET(*cpu, &original.affinity) })
        .unwrap();
    unsafe {
        libc::CPU_SET(cpu, &mut affinity);
    }
    assert_eq!(
        unsafe { libc::sched_setaffinity(0, std::mem::size_of_val(&affinity), &affinity) },
        0
    );

    let fake = FakeCarlaProcessor::new(
        FXChainType::CarlaRack,
        2,
        shoop_plugin_protocol::MAX_BLOCK_FRAMES,
    );
    let (control, mut endpoint) = spawn_processor_bridge(Box::new(fake), 48_000, 1024).unwrap();
    control.set_active(true);
    control.save_state().unwrap();
    endpoint.audio_input_mut(0).unwrap()[..32].fill(0.75);

    assert_eq!(
        unsafe { libc::sched_setscheduler(0, libc::SCHED_FIFO, &parameters) },
        0
    );
    let result = endpoint.process(32);
    original.restore_policy();
    result.unwrap();
    assert_eq!(&endpoint.audio_output(0).unwrap()[..32], &[0.75; 32]);
    assert_eq!(control.deadline_misses(), 0);
}
