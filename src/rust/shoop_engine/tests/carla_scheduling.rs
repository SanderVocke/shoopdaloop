#![cfg(all(target_os = "linux", feature = "carla"))]

use shoop_engine::carla_processor::{
    spawn_processor_bridge_with_scheduling, CarlaProcessor, CarlaProcessorInfo, FakeCarlaProcessor,
};
use shoop_engine::carla_shared_memory::SharedBlockTransport;
use shoop_engine::FXChainType;
use shoop_plugin_protocol::{BlockSequence, ProcessGeneration};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

mod backend_availability;

struct SchedulingGuard {
    affinity: libc::cpu_set_t,
    policy: libc::c_int,
    parameters: libc::sched_param,
}

impl SchedulingGuard {
    fn capture() -> Self {
        unsafe {
            let mut affinity = std::mem::zeroed();
            let mut parameters = std::mem::zeroed();
            let policy = libc::sched_getscheduler(0);
            assert!(policy >= 0);
            assert_eq!(libc::sched_getparam(0, &mut parameters), 0);
            assert_eq!(
                libc::sched_getaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &mut affinity),
                0
            );
            Self {
                affinity,
                policy,
                parameters,
            }
        }
    }

    fn cpu(&self) -> usize {
        (0..libc::CPU_SETSIZE as usize)
            .rev()
            .find(|cpu| unsafe { libc::CPU_ISSET(*cpu, &self.affinity) })
            .expect("an available CPU")
    }
}

impl Drop for SchedulingGuard {
    fn drop(&mut self) {
        unsafe {
            assert_eq!(
                libc::sched_setscheduler(0, self.policy, &self.parameters),
                0
            );
            assert_eq!(
                libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &self.affinity),
                0
            );
        }
    }
}

fn realtime_available(scheduling: &SchedulingGuard, priority: i32) -> bool {
    let parameters = libc::sched_param {
        sched_priority: priority,
    };
    if unsafe { libc::sched_setscheduler(0, libc::SCHED_FIFO, &parameters) } != 0 {
        backend_availability::require_backend(
            "realtime scheduling",
            &std::io::Error::last_os_error().to_string(),
        );
        return false;
    }
    unsafe {
        assert_eq!(
            libc::sched_setscheduler(0, scheduling.policy, &scheduling.parameters),
            0
        );
    }
    true
}

fn pin(cpu: usize) {
    unsafe {
        let mut affinity = std::mem::zeroed();
        libc::CPU_ZERO(&mut affinity);
        libc::CPU_SET(cpu, &mut affinity);
        assert_eq!(
            libc::sched_setaffinity(0, std::mem::size_of_val(&affinity), &affinity),
            0
        );
    }
}

#[shoop_wasm_test_support::shoop_test]
fn realtime_caller_allows_lower_priority_consumer_on_the_same_cpu() {
    let scheduling = SchedulingGuard::capture();
    let cpu = scheduling.cpu();
    let priority = unsafe { libc::sched_get_priority_min(libc::SCHED_FIFO) };
    assert!(priority >= 0);
    let parameters = libc::sched_param {
        sched_priority: priority,
    };
    // Probe permission before starting the consumer so a missing facility cannot strand it.
    if !realtime_available(&scheduling, priority) {
        return;
    }

    let nonce = [19; 32];
    let generation = ProcessGeneration(1);
    let mut parent = SharedBlockTransport::create(generation, &nonce).unwrap();
    let mut consumer = SharedBlockTransport::open(parent.path(), generation, &nonce).unwrap();
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let (start_tx, start_rx) = mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        pin(cpu);
        ready_tx.send(()).unwrap();
        start_rx.recv().unwrap();
        if let Some(token) = consumer.worker_take() {
            let _ = consumer.worker_complete(token, &[&[0.75; 8]], &[]);
        }
    });
    ready_rx.recv().unwrap();
    pin(cpu);
    unsafe {
        assert_eq!(
            libc::sched_setscheduler(0, libc::SCHED_FIFO, &parameters),
            0
        );
    }
    let token = parent
        .submit(BlockSequence(1), 8, &[vec![1.0; 8]], 1, &[], &[])
        .unwrap();
    let mut output = [vec![0.0; 8]];
    let mut midi = Vec::new();
    start_tx.send(()).unwrap();
    let result = parent.wait_and_copy(
        token,
        Instant::now() + Duration::from_millis(100),
        &mut output,
        &mut midi,
    );
    drop(scheduling);
    worker.join().unwrap();
    assert_eq!(
        result,
        Ok(()),
        "the realtime caller must sleep, not starve its consumer"
    );
    assert_eq!(output[0], [0.75; 8]);
}

#[derive(Debug)]
struct ObservedProcessor {
    inner: FakeCarlaProcessor,
    cpu: usize,
    dsp_priority: Arc<AtomicI32>,
    control_policy: Arc<AtomicI32>,
    deny_promotion: bool,
    filter_installed: bool,
    busy_for: Duration,
    policy_after: Arc<AtomicI32>,
    prepared_priority: Arc<AtomicI32>,
}

fn deny_realtime_promotion() -> anyhow::Result<()> {
    let statement = |code: u32, k: u32| libc::sock_filter {
        code: code as u16,
        jt: 0,
        jf: 0,
        k,
    };
    let jump = |k, jt, jf| libc::sock_filter {
        code: (libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K) as u16,
        jt,
        jf,
        k,
    };
    let mut filter = [
        statement(libc::BPF_LD | libc::BPF_W | libc::BPF_ABS, 0),
        jump(libc::SYS_sched_setscheduler as u32, 0, 4),
        statement(
            libc::BPF_LD | libc::BPF_W | libc::BPF_ABS,
            if cfg!(target_endian = "little") {
                24
            } else {
                28
            },
        ),
        jump(libc::SCHED_FIFO as u32, 1, 0),
        jump(libc::SCHED_RR as u32, 0, 1),
        statement(
            libc::BPF_RET | libc::BPF_K,
            libc::SECCOMP_RET_ERRNO | libc::EPERM as u32,
        ),
        statement(libc::BPF_RET | libc::BPF_K, libc::SECCOMP_RET_ALLOW),
    ];
    let program = libc::sock_fprog {
        len: filter.len() as u16,
        filter: filter.as_mut_ptr(),
    };
    if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0
        || unsafe { libc::prctl(libc::PR_SET_SECCOMP, libc::SECCOMP_MODE_FILTER, &program) } != 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

impl CarlaProcessor for ObservedProcessor {
    fn info(&self) -> CarlaProcessorInfo {
        self.inner.info()
    }
    fn prepare_scheduling(
        &mut self,
        _policy: i32,
        priority: i32,
        _ceiling: i32,
        _parameters: [u32; 4],
        _deadline: Instant,
    ) -> anyhow::Result<()> {
        self.control_policy
            .store(unsafe { libc::sched_getscheduler(0) }, Ordering::Release);
        self.prepared_priority.store(priority, Ordering::Release);
        Ok(())
    }
    fn set_active(&mut self, active: bool) {
        self.inner.set_active(active);
    }
    fn is_active(&self) -> bool {
        self.inner.is_active()
    }
    fn set_visible(&mut self, visible: bool) -> anyhow::Result<()> {
        if self.deny_promotion && !self.filter_installed {
            deny_realtime_promotion()?;
            self.filter_installed = true;
        }
        self.inner.set_visible(visible)
    }
    fn is_visible(&mut self) -> bool {
        self.inner.is_visible()
    }
    fn save_state(&mut self) -> anyhow::Result<String> {
        self.control_policy
            .store(unsafe { libc::sched_getscheduler(0) }, Ordering::Release);
        self.inner.save_state()
    }
    fn restore_state(&mut self, state: &str) -> anyhow::Result<()> {
        self.inner.restore_state(state)
    }
    fn audio_input_mut(&mut self, index: usize) -> Option<&mut [f32]> {
        self.inner.audio_input_mut(index)
    }
    fn audio_output(&self, index: usize) -> Option<&[f32]> {
        self.inner.audio_output(index)
    }
    fn set_midi_input_events(
        &mut self,
        index: usize,
        events: &[(u32, &[u8])],
    ) -> anyhow::Result<()> {
        self.inner.set_midi_input_events(index, events)
    }
    fn midi_output_events(&mut self, index: usize) -> anyhow::Result<Vec<(u32, Vec<u8>)>> {
        self.inner.midi_output_events(index)
    }
    fn process(&mut self, frames: usize) -> anyhow::Result<()> {
        pin(self.cpu);
        let mut parameters: libc::sched_param = unsafe { std::mem::zeroed() };
        assert_eq!(unsafe { libc::sched_getparam(0, &mut parameters) }, 0);
        self.dsp_priority
            .store(parameters.sched_priority, Ordering::Release);
        if self.deny_promotion {
            let request = libc::sched_param {
                sched_priority: unsafe { libc::sched_get_priority_min(libc::SCHED_FIFO) },
            };
            assert_eq!(
                unsafe { libc::sched_setscheduler(0, libc::SCHED_FIFO, &request) },
                -1
            );
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(libc::EPERM)
            );
        }
        let end = Instant::now() + self.busy_for;
        while Instant::now() < end {
            std::hint::spin_loop();
        }
        self.policy_after
            .store(unsafe { libc::sched_getscheduler(0) }, Ordering::Release);
        self.inner.process(frames)
    }
}

fn bridge_with_realtime_caller(allow_promotion: bool, deny_promotion: bool) {
    let scheduling = SchedulingGuard::capture();
    let priority = unsafe { libc::sched_get_priority_min(libc::SCHED_FIFO) } + 2;
    if !realtime_available(&scheduling, priority) {
        return;
    }
    let dsp_priority = Arc::new(AtomicI32::new(-1));
    let control_policy = Arc::new(AtomicI32::new(-1));
    let host = ObservedProcessor {
        inner: FakeCarlaProcessor::new(
            FXChainType::CarlaRack,
            2,
            shoop_plugin_protocol::MAX_BLOCK_FRAMES,
        ),
        cpu: scheduling.cpu(),
        dsp_priority: Arc::clone(&dsp_priority),
        control_policy: Arc::clone(&control_policy),
        deny_promotion,
        filter_installed: false,
        busy_for: Duration::ZERO,
        policy_after: Arc::new(AtomicI32::new(-1)),
        prepared_priority: Arc::new(AtomicI32::new(-1)),
    };
    let ceiling = if allow_promotion { priority } else { 0 };
    let (control, mut endpoint) =
        spawn_processor_bridge_with_scheduling(Box::new(host), 1000, 100, ceiling).unwrap();
    control.set_active(true);
    control.set_visible(false).unwrap();
    endpoint.audio_input_mut(0).unwrap()[..8].fill(0.75);
    pin(scheduling.cpu());
    let parameters = libc::sched_param {
        sched_priority: priority,
    };
    assert_eq!(
        unsafe { libc::sched_setscheduler(0, libc::SCHED_FIFO, &parameters) },
        0
    );
    endpoint.process(8).unwrap();
    drop(scheduling);
    assert_eq!(endpoint.audio_output(0).unwrap()[..8], [0.75; 8]);
    assert_eq!(
        dsp_priority.load(Ordering::Acquire),
        if allow_promotion && !deny_promotion {
            priority - 1
        } else {
            0
        }
    );
    control.save_state().unwrap();
    assert_eq!(control_policy.load(Ordering::Acquire), libc::SCHED_OTHER);
}

#[shoop_wasm_test_support::shoop_test]
fn bridge_promotes_dsp_below_its_caller_and_demotes_before_control() {
    bridge_with_realtime_caller(true, false);
}

#[shoop_wasm_test_support::shoop_test]
fn exhausted_backend_priority_uses_safe_sleeping_fallback() {
    bridge_with_realtime_caller(false, false);
}

#[shoop_wasm_test_support::shoop_test]
fn kernel_denied_scheduling_uses_safe_sleeping_fallback() {
    bridge_with_realtime_caller(true, true);
}

#[shoop_wasm_test_support::shoop_test]
fn startup_respects_backend_ceiling_and_prepares_the_child_after_demoting() {
    let scheduling = SchedulingGuard::capture();
    let priority = unsafe { libc::sched_get_priority_min(libc::SCHED_FIFO) } + 4;
    if !realtime_available(&scheduling, priority) {
        return;
    }
    let control_policy = Arc::new(AtomicI32::new(-1));
    let prepared = Arc::new(AtomicI32::new(-1));
    let host = ObservedProcessor {
        inner: FakeCarlaProcessor::new(
            FXChainType::CarlaRack,
            2,
            shoop_plugin_protocol::MAX_BLOCK_FRAMES,
        ),
        cpu: scheduling.cpu(),
        dsp_priority: Arc::new(AtomicI32::new(-1)),
        control_policy: Arc::clone(&control_policy),
        deny_promotion: false,
        filter_installed: false,
        busy_for: Duration::ZERO,
        policy_after: Arc::new(AtomicI32::new(-1)),
        prepared_priority: Arc::clone(&prepared),
    };
    let parameters = libc::sched_param {
        sched_priority: priority,
    };
    assert_eq!(
        unsafe { libc::sched_setscheduler(0, libc::SCHED_FIFO, &parameters) },
        0
    );
    let ceiling = priority - 2;
    let bridge =
        spawn_processor_bridge_with_scheduling(Box::new(host), 1000, 100, ceiling).unwrap();
    drop(scheduling);
    assert_eq!(prepared.load(Ordering::Acquire), ceiling - 2);
    assert_eq!(control_policy.load(Ordering::Acquire), libc::SCHED_OTHER);
    drop(bridge);
}

#[shoop_wasm_test_support::shoop_test]
fn cpu_busy_plugin_is_demoted_so_the_caller_and_other_plugins_can_progress() {
    let scheduling = SchedulingGuard::capture();
    let priority = unsafe { libc::sched_get_priority_min(libc::SCHED_FIFO) } + 2;
    if !realtime_available(&scheduling, priority) {
        return;
    }
    pin(scheduling.cpu());
    let bad_after = Arc::new(AtomicI32::new(-1));
    let mut endpoints = Vec::new();
    let mut controls = Vec::new();
    for busy in [true, false] {
        let host = ObservedProcessor {
            inner: FakeCarlaProcessor::new(
                FXChainType::CarlaRack,
                2,
                shoop_plugin_protocol::MAX_BLOCK_FRAMES,
            ),
            cpu: scheduling.cpu(),
            dsp_priority: Arc::new(AtomicI32::new(-1)),
            control_policy: Arc::new(AtomicI32::new(-1)),
            deny_promotion: false,
            filter_installed: false,
            busy_for: if busy {
                Duration::from_millis(500)
            } else {
                Duration::ZERO
            },
            policy_after: if busy {
                Arc::clone(&bad_after)
            } else {
                Arc::new(AtomicI32::new(-1))
            },
            prepared_priority: Arc::new(AtomicI32::new(-1)),
        };
        let (control, mut endpoint) =
            spawn_processor_bridge_with_scheduling(Box::new(host), 1000, 100, priority).unwrap();
        control.set_active(true);
        control.set_visible(false).unwrap();
        endpoint.audio_input_mut(0).unwrap()[..8].fill(0.75);
        endpoints.push(endpoint);
        controls.push(control);
    }
    let parameters = libc::sched_param {
        sched_priority: priority,
    };
    assert_eq!(
        unsafe { libc::sched_setscheduler(0, libc::SCHED_FIFO, &parameters) },
        0
    );
    endpoints[0]
        .process_until(8, Instant::now() + Duration::from_millis(100))
        .unwrap();
    endpoints[1]
        .process_until(8, Instant::now() + Duration::from_millis(100))
        .unwrap();
    drop(scheduling);
    let failed = endpoints[0].audio_output(0).unwrap()[..8].to_vec();
    let healthy = endpoints[1].audio_output(0).unwrap()[..8].to_vec();
    drop(controls);
    assert_eq!(failed, [0.0; 8]);
    assert_eq!(
        healthy, [0.75; 8],
        "a CPU-busy plugin must not starve a healthy Carla worker"
    );
    assert_eq!(bad_after.load(Ordering::Acquire), libc::SCHED_OTHER);
}
