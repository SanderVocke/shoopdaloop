use anyhow::{ensure, Result};
use rt_fpga_compiler::{loading::Layout, LogicalStateProgram};
use rt_fpga_rtl::{
    host::control::descriptor,
    target::{ProgramImage, RtlTarget},
};
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock},
    host::{Phase, Renderer, Report},
    host_adapter::QuantumAdapter,
    Error, Frame,
};
use shoop_rt_host_reference::{ProgramLoader, Status};
use shoop_rt_protocol::{
    immediate::SwitchProgram, ClientMessage, DriverKind, DriverRunState, DriverStatus,
};
use shoop_rt_transport::carrier::{CarrierError, Endpoint as Carrier, Outbox};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc,
    },
    thread::{self, JoinHandle, Thread},
    time::Duration,
};

pub(super) const MAX: usize = 128;
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);
pub(super) struct Block {
    pub generation: u64,
    pub frame: u64,
    pub input: [f32; MAX],
    pub output: [[f32; MAX]; 2],
}
impl Default for Block {
    fn default() -> Self {
        Self {
            generation: 1,
            frame: 0,
            input: [0.; MAX],
            output: [[0.; MAX]; 2],
        }
    }
}
pub(super) struct Shared {
    pub stop: AtomicBool,
    pub failed: AtomicBool,
    pub status: Arc<Status>,
}
impl Shared {
    pub fn fail(&self) {
        if self.failed.swap(true, Ordering::AcqRel) {
            return;
        }
        self.status.xrun();
        let mut d = self.status.driver();
        d.state = DriverRunState::Failed;
        self.status.update_driver(d);
    }
}
fn guarded(shared: &Shared, work: impl FnOnce() -> Result<()>) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
    if !matches!(result, Ok(Ok(()))) && !shared.stop.load(Ordering::Acquire) {
        shared.fail();
    }
}
pub struct Images {
    uploads: rtrb::Producer<ProgramImage>,
    ready: rtrb::Consumer<(u64, bool)>,
    retired: rtrb::Consumer<ProgramImage>,
    wake: Thread,
}
impl ProgramLoader for Images {
    fn limits(&self) -> rt_fpga_compiler::ProgramLimits {
        shoop_rt_host_reference::program_limits()
    }
    fn stage_program(
        &mut self,
        logical: &LogicalStateProgram,
        layout: Layout,
    ) -> Result<SwitchProgram> {
        let views = layout.media_views(logical);
        let image = RtlTarget {
            capacities: self.limits(),
            media_views: &views,
            state_base: 0,
        }
        .build_image(logical, layout)?;
        let descriptor = descriptor(&image);
        self.uploads
            .push(image)
            .map_err(|_| anyhow::anyhow!("RTL upload queue is full"))?;
        self.wake.unpark();
        Ok(descriptor)
    }
    fn program_ready(&mut self) -> Option<(u64, bool)> {
        self.ready.pop().ok()
    }
    fn reclaim_programs(&mut self) {
        while self.retired.pop().is_ok() {}
    }
}
pub struct Endpoint {
    inner: shoop_rt_host_reference::Endpoint<Images>,
    wake: Thread,
}
impl Endpoint {
    pub fn snapshot_capacity(&self) -> usize {
        self.inner.snapshot_capacity()
    }
    pub fn read_snapshot(&self, bytes: &mut [u8]) -> Option<(u32, usize)> {
        self.inner.read_snapshot(bytes)
    }
}
impl Carrier for Endpoint {
    fn try_deliver(&mut self, m: ClientMessage, out: &mut Outbox) -> Result<(), CarrierError> {
        let result = self.inner.try_deliver(m, out);
        self.wake.unpark();
        result
    }
    fn deliver(&mut self, m: ClientMessage, out: &mut Outbox) {
        let _ = self.try_deliver(m, out);
    }
    fn poll(&mut self, out: &mut Outbox) {
        self.inner.poll(out);
        self.wake.unpark();
    }
}
pub struct Prepared {
    pub endpoint: Endpoint,
    pub renderer: QuantumAdapter<Bridge>,
    pub status: Arc<Status>,
}
pub fn prepare(
    quantum: u32,
    driver: DriverStatus,
    memory_mb: u32,
    max_callback: usize,
) -> Result<Prepared> {
    ensure!(
        cfg!(target_os = "linux"),
        "Marlin hosting is currently validated on Linux; use a Linux simulator host"
    );
    ensure!(
        matches!(quantum, 1 | 16 | 32 | 64 | 128) && driver.sample_rate > 0,
        "invalid RTL geometry"
    );
    ensure!(
        memory_mb > 0 && memory_mb <= 4096,
        "RTL memory must be 1..4096 MiB"
    );
    let root = rt_fpga_rtl::root();
    ensure!(
        root.join("fpga/state_processor/rtl/state_processor.sv")
            .is_file(),
        "missing FPGA RTL sources at {}; set SHOOP_FPGA_SOURCE_ROOT to the source checkout",
        root.display()
    );
    let mut generation = NEXT_GENERATION.load(Ordering::Relaxed);
    loop {
        let next = generation
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("RTL runtime generations exhausted"))?;
        match NEXT_GENERATION.compare_exchange_weak(
            generation,
            next,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(value) => generation = value,
        }
    }
    let wait = driver.kind == DriverKind::Dummy;
    let depth = if wait {
        0
    } else {
        max_callback.div_ceil(quantum as usize) + 1
    };
    let capacity = depth + 3;
    let latency = (depth + 1)
        .checked_mul(quantum as usize)
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| anyhow::anyhow!("RTL bridge capacity overflow"))?;
    let (control, execution) = shoop_rt_host_reference::prepare_control(quantum, driver, latency)?;
    let (uploads, upload_rx) = rtrb::RingBuffer::new(2);
    let (ready_tx, ready) = rtrb::RingBuffer::new(2);
    let (retire_tx, retired) = rtrb::RingBuffer::new(2);
    let (requests, request_rx) = rtrb::RingBuffer::new(capacity);
    let (results_tx, results) = rtrb::RingBuffer::new(capacity);
    let status = Arc::new(Status::new(driver));
    let shared = Arc::new(Shared {
        stop: AtomicBool::new(false),
        failed: AtomicBool::new(false),
        status: status.clone(),
    });
    let worker = shared.clone();
    let (start_tx, start_rx) = mpsc::sync_channel(1);
    let join = thread::Builder::new()
        .name("shoop-marlin".into())
        .spawn(move || {
            guarded(&worker, || {
                super::owner::run(
                    quantum, memory_mb, &worker, execution, upload_rx, ready_tx, retire_tx,
                    request_rx, results_tx, start_tx,
                )
            });
        })?;
    let wake = join.thread().clone();
    match start_rx.recv() {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            let _ = join.join();
            anyhow::bail!("RTL preparation: {error}");
        }
        Err(_) => {
            let _ = join.join();
            anyhow::bail!("RTL owner failed during preparation");
        }
    }
    let images = Images {
        uploads,
        ready,
        retired,
        wake: wake.clone(),
    };
    let endpoint = Endpoint {
        inner: shoop_rt_host_reference::Endpoint::new(control, images, quantum, status.clone()),
        wake: wake.clone(),
    };
    let bridge = Bridge {
        requests,
        results,
        free: (0..capacity)
            .map(|_| Block {
                generation,
                ..Default::default()
            })
            .collect(),
        generation,
        shared,
        wake,
        join: Some(join),
        frame: 0,
        quantum: quantum as usize,
        depth,
        wait,
    };
    Ok(Prepared {
        endpoint,
        renderer: QuantumAdapter::new(bridge, quantum as usize, 1, 2)?,
        status,
    })
}
#[derive(Clone)]
pub struct StopHandle {
    shared: Arc<Shared>,
    wake: Thread,
}
impl StopHandle {
    pub fn request(&self) {
        self.shared.stop.store(true, Ordering::Release);
        self.wake.unpark();
    }
}
pub struct Bridge {
    requests: rtrb::Producer<Block>,
    results: rtrb::Consumer<Block>,
    free: Vec<Block>,
    shared: Arc<Shared>,
    wake: Thread,
    join: Option<JoinHandle<()>>,
    frame: u64,
    generation: u64,
    quantum: usize,
    depth: usize,
    wait: bool,
}
impl Bridge {
    pub fn stop_handle(&self) -> StopHandle {
        StopHandle {
            shared: self.shared.clone(),
            wake: self.wake.clone(),
        }
    }
    pub fn stop(&mut self) -> Result<(), Error> {
        self.shared.stop.store(true, Ordering::Release);
        self.wake.unpark();
        if let Some(join) = self.join.take() {
            join.join().map_err(|_| Error::InvalidState)?;
        }
        Ok(())
    }
    fn exchange(&mut self, audio: AudioBlock<'_, '_>) -> Result<(), Error> {
        if self.shared.stop.load(Ordering::Acquire) {
            return Err(Error::Disconnected);
        }
        if self.shared.failed.load(Ordering::Acquire) {
            return Err(Error::InvalidState);
        }
        let mut block = self.free.pop().ok_or(Error::Full)?;
        block.frame = self.frame;
        for i in 0..self.quantum {
            block.input[i] = audio.inputs.first().map_or(0., |p| p[i]);
        }
        self.requests.push(block).map_err(|_| Error::Full)?;
        self.wake.unpark();
        if self.wait || self.frame >= self.depth as u64 * self.quantum as u64 {
            let block = loop {
                if let Ok(block) = self.results.pop() {
                    break block;
                }
                if !self.wait {
                    return Err(Error::Full);
                }
                if self.shared.failed.load(Ordering::Acquire)
                    || self.shared.stop.load(Ordering::Acquire)
                {
                    return Err(Error::InvalidState);
                }
                thread::park_timeout(Duration::from_millis(1));
            };
            let expected = if self.wait {
                self.frame
            } else {
                self.frame - self.depth as u64 * self.quantum as u64
            };
            if block.generation != self.generation || block.frame != expected {
                return Err(Error::InvalidFrame);
            }
            for (port, out) in audio.outputs.iter_mut().enumerate() {
                if port < 2 {
                    out[..self.quantum].copy_from_slice(&block.output[port][..self.quantum]);
                } else {
                    out[..self.quantum].fill(0.);
                }
            }
            self.free.push(block);
        } else {
            for out in audio.outputs {
                out[..self.quantum].fill(0.);
            }
        }
        self.frame += self.quantum as u64;
        Ok(())
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        self.wake.unpark();
    }
}
impl Renderer for Bridge {
    fn next_frame(&self) -> Frame {
        Frame(self.frame)
    }
    fn process(&mut self, audio: AudioBlock<'_, '_>, midi: MidiBlock<'_>) -> Result<Report, Error> {
        if audio.frames != self.quantum {
            return Err(Error::InvalidShape);
        }
        for out in midi.outputs {
            out.clear();
        }
        let result = self.exchange(AudioBlock {
            frame: audio.frame,
            frames: audio.frames,
            inputs: audio.inputs,
            outputs: &mut *audio.outputs,
        });
        if let Err(error) = result {
            for out in audio.outputs {
                out[..audio.frames].fill(0.);
            }
            if self.shared.stop.load(Ordering::Acquire) {
                return Ok(Report {
                    next_frame: self.next_frame(),
                    phase: Phase::Disconnected,
                    recovery_packets: 0,
                });
            }
            self.shared.fail();
            return Err(error);
        }
        Ok(Report {
            next_frame: self.next_frame(),
            phase: Phase::Running,
            recovery_packets: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};
    fn fixture(wait: bool) -> (Bridge, rtrb::Consumer<Block>, rtrb::Producer<Block>) {
        let (tx, rx) = rtrb::RingBuffer::new(4);
        let (out, results) = rtrb::RingBuffer::new(4);
        let status = Arc::new(Status::new(DriverStatus {
            kind: DriverKind::Cpal,
            state: DriverRunState::Running,
            sample_rate: 48000,
            buffer_frames: 1,
        }));
        (
            Bridge {
                requests: tx,
                results,
                free: (0..4).map(|_| Block::default()).collect(),
                shared: Arc::new(Shared {
                    stop: AtomicBool::new(false),
                    failed: AtomicBool::new(false),
                    status,
                }),
                wake: thread::current(),
                join: None,
                frame: 0,
                generation: 1,
                quantum: 1,
                depth: 1,
                wait,
            },
            rx,
            out,
        )
    }
    fn step(bridge: &mut Bridge) -> (Result<Report, Error>, f32) {
        let mut output = [123.];
        let result = assert_no_heap(|| {
            bridge.process(
                AudioBlock {
                    frame: bridge.next_frame(),
                    frames: 1,
                    inputs: &[&[0.5]],
                    outputs: &mut [&mut output],
                },
                MidiBlock {
                    inputs: &[],
                    outputs: &mut [],
                },
            )
        });
        (result, output[0])
    }
    #[shoop_test(no_wasm = "native callback bridge")]
    fn physical_bridge_primes_orders_and_recycles_without_allocation() {
        let (mut bridge, mut input, mut output) = fixture(false);
        assert_eq!(step(&mut bridge).1, 0.);
        for frame in 0..100 {
            let mut block = input.pop().unwrap();
            assert_eq!(block.frame, frame);
            assert_eq!(block.input[0], 0.5);
            block.output[0][0] = frame as f32 + 1.;
            output.push(block).ok().unwrap();
            let (result, sample) = step(&mut bridge);
            result.unwrap();
            assert_eq!(sample, frame as f32 + 1.);
        }
    }
    #[shoop_test(no_wasm = "native callback bridge")]
    fn delayed_owner_and_stale_output_latch_a_silent_fault() {
        let (mut bridge, _, _) = fixture(false);
        step(&mut bridge).0.unwrap();
        let (result, sample) = step(&mut bridge);
        assert_eq!(result, Err(Error::Full));
        assert_eq!(sample, 0.);
        assert_eq!(bridge.shared.status.driver().state, DriverRunState::Failed);
        assert!(step(&mut bridge).0.is_err());
        let (mut bridge, mut input, mut output) = fixture(false);
        step(&mut bridge).0.unwrap();
        let mut block = input.pop().unwrap();
        block.generation = 9;
        output.push(block).ok().unwrap();
        assert_eq!(step(&mut bridge).0, Err(Error::InvalidFrame));
    }
    #[shoop_test(no_wasm = "native owner failure")]
    fn owner_panic_latches_a_fault_and_releases_a_dummy_wait() {
        let (mut bridge, _, _) = fixture(true);
        let shared = bridge.shared.clone();
        let join =
            thread::spawn(move || guarded(&shared, || panic!("injected simulator owner panic")));
        join.join().unwrap();
        let mut running = bridge.shared.status.driver();
        running.state = DriverRunState::Running;
        bridge.shared.status.update_driver(running);
        assert_eq!(step(&mut bridge).0, Err(Error::InvalidState));
        assert_eq!(bridge.shared.status.driver().state, DriverRunState::Failed);
    }
    #[shoop_test(no_wasm = "native non-realtime dummy wait")]
    fn stop_bypasses_a_dummy_wait_without_output_or_control_draining() {
        let (mut bridge, _, _) = fixture(true);
        let shared = bridge.shared.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        let join = thread::spawn(move || {
            let result = step(&mut bridge).0;
            tx.send(result).unwrap();
        });
        shared.stop.store(true, Ordering::Release);
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(1))
                .unwrap()
                .unwrap()
                .phase,
            Phase::Disconnected
        );
        join.join().unwrap();
    }
}
