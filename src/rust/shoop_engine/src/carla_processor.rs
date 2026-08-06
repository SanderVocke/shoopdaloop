use crate::FXChainType;
use anyhow::Result;
use std::fmt::Debug;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CarlaProcessorInfo {
    pub chain_type: FXChainType,
    pub audio_inputs: usize,
    pub audio_outputs: usize,
    pub midi_inputs: usize,
    pub midi_outputs: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CarlaProcessorLifecycle {
    #[default]
    Stopped,
    Starting,
    Running,
    Crashed,
    Restarting,
    Unavailable,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CarlaGenerationLog {
    pub generation: u64,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_dropped_bytes: u64,
    pub stderr_dropped_bytes: u64,
}

#[derive(Debug)]
pub struct CarlaMidiBuffer {
    events: Vec<shoop_plugin_protocol::MidiEvent>,
    count: usize,
}

impl CarlaMidiBuffer {
    pub fn new(event_capacity: usize, event_byte_capacity: usize) -> Self {
        Self {
            events: (0..event_capacity)
                .map(|_| shoop_plugin_protocol::MidiEvent {
                    frame_offset: 0,
                    data: Vec::with_capacity(event_byte_capacity),
                })
                .collect(),
            count: 0,
        }
    }

    pub fn clear(&mut self) {
        self.count = 0;
    }

    pub fn push(&mut self, frame_offset: u32, data: &[u8]) -> Result<()> {
        let event = self
            .events
            .get_mut(self.count)
            .ok_or_else(|| anyhow::anyhow!("Carla MIDI event capacity exceeded"))?;
        if data.len() > event.data.capacity() {
            anyhow::bail!("Carla MIDI event byte capacity exceeded");
        }
        event.frame_offset = frame_offset;
        event.data.clear();
        event.data.extend_from_slice(data);
        self.count += 1;
        Ok(())
    }

    pub fn as_slice(&self) -> &[shoop_plugin_protocol::MidiEvent] {
        &self.events[..self.count]
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn storage_mut(&mut self) -> (&mut [shoop_plugin_protocol::MidiEvent], &mut usize) {
        (&mut self.events, &mut self.count)
    }
}

pub trait CarlaProcessor: Send + Debug {
    fn info(&self) -> CarlaProcessorInfo;
    fn is_ready(&mut self) -> bool {
        true
    }
    fn lifecycle(&self) -> CarlaProcessorLifecycle {
        CarlaProcessorLifecycle::Running
    }
    fn generation(&self) -> u64 {
        0
    }
    fn exit_kind(&self) -> shoop_plugin_protocol::WorkerExitKind {
        shoop_plugin_protocol::WorkerExitKind::None
    }
    fn crash_summary(&self) -> Option<String> {
        None
    }
    fn generation_logs(&self) -> Vec<CarlaGenerationLog> {
        Vec::new()
    }
    fn clear_logs(&mut self) {}
    fn toggle_or_recover(&mut self) -> Result<()> {
        let visible = self.is_visible();
        self.set_visible(!visible)
    }
    fn set_active(&mut self, active: bool);
    fn is_active(&self) -> bool;
    fn set_visible(&mut self, visible: bool) -> Result<()>;
    fn is_visible(&mut self) -> bool;
    fn save_state(&mut self) -> Result<String>;
    fn restore_state(&mut self, state: &str) -> Result<()>;
    fn audio_input_mut(&mut self, index: usize) -> Option<&mut [f32]>;
    fn audio_output(&self, index: usize) -> Option<&[f32]>;
    fn set_midi_input_events(&mut self, index: usize, events: &[(u32, &[u8])]) -> Result<()>;
    fn midi_output_events(&mut self, index: usize) -> Result<Vec<(u32, Vec<u8>)>>;
    fn fill_midi_output_events(
        &mut self,
        index: usize,
        destination: &mut CarlaMidiBuffer,
    ) -> Result<()> {
        destination.clear();
        for (frame_offset, data) in self.midi_output_events(index)? {
            destination.push(frame_offset, &data)?;
        }
        Ok(())
    }
    fn process(&mut self, frames: usize) -> Result<()>;
}

#[cfg(not(target_arch = "wasm32"))]
mod bridge {
    use super::*;
    use crate::carla_shared_memory::SharedBlockTransport;
    use crate::realtime_lock_guard::Mutex;
    use anyhow::anyhow;
    use arc_swap::ArcSwapOption;
    use shoop_plugin_protocol::{
        BlockSequence, MidiEvent, ProcessGeneration, MAX_AUDIO_CHANNELS, MAX_BLOCK_FRAMES,
        MAX_MIDI_EVENTS_PER_BLOCK,
    };
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
    use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
    use std::sync::Arc;
    use std::thread::JoinHandle;
    use std::time::Instant;

    const BRIDGE_COMMAND_CAPACITY: usize = 32;

    #[derive(Debug)]
    struct BridgeSnapshot {
        ready: AtomicBool,
        active: Arc<AtomicBool>,
        visible: AtomicBool,
        lifecycle: AtomicU8,
        generation: AtomicU64,
        exit_kind: AtomicU8,
        deadline_misses: Arc<AtomicU64>,
        midi_input_overflows: Arc<AtomicU64>,
        midi_output_overflows: Arc<AtomicU64>,
        stale_completions: Arc<AtomicU64>,
        crash_summary: ArcSwapOption<String>,
    }

    impl BridgeSnapshot {
        fn new(host: &mut dyn CarlaProcessor) -> Self {
            Self {
                ready: AtomicBool::new(host.is_ready()),
                active: Arc::new(AtomicBool::new(host.is_active())),
                visible: AtomicBool::new(host.is_visible()),
                lifecycle: AtomicU8::new(host.lifecycle() as u8),
                generation: AtomicU64::new(host.generation()),
                exit_kind: AtomicU8::new(host.exit_kind() as u8),
                deadline_misses: Arc::new(AtomicU64::new(0)),
                midi_input_overflows: Arc::new(AtomicU64::new(0)),
                midi_output_overflows: Arc::new(AtomicU64::new(0)),
                stale_completions: Arc::new(AtomicU64::new(0)),
                crash_summary: ArcSwapOption::from(host.crash_summary().map(Arc::new)),
            }
        }

        fn publish_health(&self, host: &mut dyn CarlaProcessor) {
            self.ready.store(host.is_ready(), Ordering::Release);
            self.lifecycle
                .store(host.lifecycle() as u8, Ordering::Release);
            self.generation.store(host.generation(), Ordering::Release);
            self.exit_kind
                .store(host.exit_kind() as u8, Ordering::Release);
            self.crash_summary.store(host.crash_summary().map(Arc::new));
        }
    }

    type UnitReply = SyncSender<std::result::Result<(), String>>;
    type StringReply = SyncSender<std::result::Result<String, String>>;
    type LogReply = SyncSender<Vec<CarlaGenerationLog>>;

    #[derive(Debug)]
    enum BridgeCommand {
        SetActive(bool),
        SetVisible(bool, UnitReply),
        ToggleOrRecover(UnitReply),
        SaveState(StringReply),
        RestoreState(String, UnitReply),
        GenerationLogs(LogReply),
        ClearLogs,
        Shutdown,
    }

    #[derive(Debug)]
    struct BridgeControl {
        sender: SyncSender<BridgeCommand>,
        snapshot: Arc<BridgeSnapshot>,
        thread: Mutex<Option<JoinHandle<()>>>,
        wake: std::thread::Thread,
    }

    impl Drop for BridgeControl {
        fn drop(&mut self) {
            let _ = self.sender.send(BridgeCommand::Shutdown);
            self.wake.unpark();
            if let Some(thread) = self.thread.lock().unwrap_or_else(|e| e.into_inner()).take() {
                let _ = thread.join();
            }
        }
    }

    /// Non-real-time handle for a processor owned by a dedicated bridge thread.
    ///
    /// Clones share bounded control commands and atomically published observations;
    /// they never lock or otherwise borrow the callback-owned realtime endpoint.
    #[derive(Clone, Debug)]
    pub struct CarlaControlHandle {
        info: CarlaProcessorInfo,
        control: Arc<BridgeControl>,
    }

    impl CarlaControlHandle {
        fn request_unit(&self, make: impl FnOnce(UnitReply) -> BridgeCommand) -> Result<()> {
            let (sender, receiver) = sync_channel(1);
            self.control
                .sender
                .send(make(sender))
                .map_err(|_| anyhow!("Carla processor bridge stopped"))?;
            self.control.wake.unpark();
            receiver
                .recv()
                .map_err(|_| anyhow!("Carla processor bridge stopped"))?
                .map_err(anyhow::Error::msg)
        }

        pub fn info(&self) -> CarlaProcessorInfo {
            self.info
        }

        pub fn is_ready(&self) -> bool {
            self.control.snapshot.ready.load(Ordering::Acquire)
        }

        pub fn lifecycle(&self) -> CarlaProcessorLifecycle {
            match self.control.snapshot.lifecycle.load(Ordering::Acquire) {
                value if value == CarlaProcessorLifecycle::Starting as u8 => {
                    CarlaProcessorLifecycle::Starting
                }
                value if value == CarlaProcessorLifecycle::Running as u8 => {
                    CarlaProcessorLifecycle::Running
                }
                value if value == CarlaProcessorLifecycle::Crashed as u8 => {
                    CarlaProcessorLifecycle::Crashed
                }
                value if value == CarlaProcessorLifecycle::Restarting as u8 => {
                    CarlaProcessorLifecycle::Restarting
                }
                value if value == CarlaProcessorLifecycle::Unavailable as u8 => {
                    CarlaProcessorLifecycle::Unavailable
                }
                _ => CarlaProcessorLifecycle::Stopped,
            }
        }

        pub fn generation(&self) -> u64 {
            self.control.snapshot.generation.load(Ordering::Acquire)
        }

        pub fn exit_kind(&self) -> shoop_plugin_protocol::WorkerExitKind {
            match self.control.snapshot.exit_kind.load(Ordering::Acquire) {
                value if value == shoop_plugin_protocol::WorkerExitKind::Requested as u8 => {
                    shoop_plugin_protocol::WorkerExitKind::Requested
                }
                value if value == shoop_plugin_protocol::WorkerExitKind::UiClosed as u8 => {
                    shoop_plugin_protocol::WorkerExitKind::UiClosed
                }
                value if value == shoop_plugin_protocol::WorkerExitKind::StartupFailure as u8 => {
                    shoop_plugin_protocol::WorkerExitKind::StartupFailure
                }
                value if value == shoop_plugin_protocol::WorkerExitKind::ProtocolFailure as u8 => {
                    shoop_plugin_protocol::WorkerExitKind::ProtocolFailure
                }
                value if value == shoop_plugin_protocol::WorkerExitKind::UnexpectedExit as u8 => {
                    shoop_plugin_protocol::WorkerExitKind::UnexpectedExit
                }
                value if value == shoop_plugin_protocol::WorkerExitKind::Unresponsive as u8 => {
                    shoop_plugin_protocol::WorkerExitKind::Unresponsive
                }
                value
                    if value == shoop_plugin_protocol::WorkerExitKind::ParentDisconnected as u8 =>
                {
                    shoop_plugin_protocol::WorkerExitKind::ParentDisconnected
                }
                _ => shoop_plugin_protocol::WorkerExitKind::None,
            }
        }

        pub fn deadline_misses(&self) -> u64 {
            self.control
                .snapshot
                .deadline_misses
                .load(Ordering::Acquire)
        }

        pub fn midi_input_overflows(&self) -> u64 {
            self.control
                .snapshot
                .midi_input_overflows
                .load(Ordering::Acquire)
        }

        pub fn midi_output_overflows(&self) -> u64 {
            self.control
                .snapshot
                .midi_output_overflows
                .load(Ordering::Acquire)
        }

        pub fn stale_completions(&self) -> u64 {
            self.control
                .snapshot
                .stale_completions
                .load(Ordering::Acquire)
        }

        pub fn crash_summary(&self) -> Option<String> {
            self.control
                .snapshot
                .crash_summary
                .load_full()
                .map(|summary| (*summary).clone())
        }

        pub fn set_active(&self, active: bool) {
            // Publish desired activity immediately, then apply it in FIFO order on
            // the bridge thread. This keeps QML state deterministic without sharing
            // the callback endpoint or making the callback consume control traffic.
            self.control
                .snapshot
                .active
                .store(active, Ordering::Release);
            if self
                .control
                .sender
                .send(BridgeCommand::SetActive(active))
                .is_err()
            {
                self.control.snapshot.ready.store(false, Ordering::Release);
            } else {
                self.control.wake.unpark();
            }
        }

        pub fn is_active(&self) -> bool {
            self.control.snapshot.active.load(Ordering::Acquire)
        }

        pub fn set_visible(&self, visible: bool) -> Result<()> {
            self.request_unit(|reply| BridgeCommand::SetVisible(visible, reply))?;
            self.control
                .snapshot
                .visible
                .store(visible, Ordering::Release);
            Ok(())
        }

        pub fn is_visible(&self) -> bool {
            self.control.snapshot.visible.load(Ordering::Acquire)
        }

        pub fn toggle_or_recover(&self) -> Result<()> {
            self.request_unit(BridgeCommand::ToggleOrRecover)
        }

        pub fn save_state(&self) -> Result<String> {
            let (sender, receiver) = sync_channel(1);
            self.control
                .sender
                .send(BridgeCommand::SaveState(sender))
                .map_err(|_| anyhow!("Carla processor bridge stopped"))?;
            self.control.wake.unpark();
            receiver
                .recv()
                .map_err(|_| anyhow!("Carla processor bridge stopped"))?
                .map_err(anyhow::Error::msg)
        }

        pub fn restore_state(&self, state: &str) -> Result<()> {
            self.request_unit(|reply| BridgeCommand::RestoreState(state.to_owned(), reply))
        }

        pub fn generation_logs(&self) -> Vec<CarlaGenerationLog> {
            let (sender, receiver) = sync_channel(1);
            if self
                .control
                .sender
                .send(BridgeCommand::GenerationLogs(sender))
                .is_err()
            {
                return Vec::new();
            }
            self.control.wake.unpark();
            receiver.recv().unwrap_or_default()
        }

        pub fn clear_logs(&self) {
            if self.control.sender.send(BridgeCommand::ClearLogs).is_ok() {
                self.control.wake.unpark();
            }
        }
    }

    /// Single-owner endpoint installed directly in the engine session.
    /// Its processing path accesses only preallocated buffers, atomics, and the
    /// bounded shared-memory transport; it has no ordinary mutex or control socket.
    #[derive(Debug)]
    pub struct CarlaRealtimeProcessor {
        info: CarlaProcessorInfo,
        transport: SharedBlockTransport,
        active: Arc<AtomicBool>,
        wake: std::thread::Thread,
        sequence: u64,
        timeout: Duration,
        audio_inputs: Vec<Vec<f32>>,
        audio_outputs: Vec<Vec<f32>>,
        midi_inputs: Vec<Vec<(u32, Vec<u8>)>>,
        midi_input_counts: Vec<usize>,
        midi_outputs: Vec<Vec<(u32, Vec<u8>)>>,
        midi_output_counts: Vec<usize>,
        shared_midi_outputs: Vec<MidiEvent>,
        shared_midi_output_count: usize,
        deadline_misses: Arc<AtomicU64>,
        midi_input_overflows: Arc<AtomicU64>,
        midi_output_overflows: Arc<AtomicU64>,
        stale_completions: Arc<AtomicU64>,
    }

    impl CarlaProcessor for CarlaRealtimeProcessor {
        fn info(&self) -> CarlaProcessorInfo {
            self.info
        }
        fn is_ready(&mut self) -> bool {
            true
        }
        fn set_active(&mut self, active: bool) {
            self.active.store(active, Ordering::Release);
        }
        fn is_active(&self) -> bool {
            self.active.load(Ordering::Acquire)
        }
        fn set_visible(&mut self, _visible: bool) -> Result<()> {
            Ok(())
        }
        fn is_visible(&mut self) -> bool {
            false
        }
        fn save_state(&mut self) -> Result<String> {
            Err(anyhow!("state belongs to bridge control"))
        }
        fn restore_state(&mut self, _state: &str) -> Result<()> {
            Err(anyhow!("state belongs to bridge control"))
        }
        fn audio_input_mut(&mut self, index: usize) -> Option<&mut [f32]> {
            self.audio_inputs.get_mut(index).map(Vec::as_mut_slice)
        }
        fn audio_output(&self, index: usize) -> Option<&[f32]> {
            self.audio_outputs.get(index).map(Vec::as_slice)
        }
        fn set_midi_input_events(&mut self, index: usize, events: &[(u32, &[u8])]) -> Result<()> {
            let destination = self
                .midi_inputs
                .get_mut(index)
                .ok_or_else(|| anyhow!("no bridge MIDI input {index}"))?;
            let mut count = 0;
            let mut overflows = 0_u64;
            for (offset, bytes) in events {
                if count == MAX_MIDI_EVENTS_PER_BLOCK
                    || bytes.len() > crate::midi_storage::MAX_MSG_BYTES
                {
                    overflows += 1;
                    continue;
                }
                let slot = &mut destination[count];
                slot.0 = *offset;
                slot.1.clear();
                slot.1.extend_from_slice(bytes);
                count += 1;
            }
            self.midi_input_counts[index] = count;
            if overflows > 0 {
                let total = self
                    .midi_input_overflows
                    .fetch_add(overflows, Ordering::Relaxed)
                    + overflows;
                shoop_tracing::realtime_plot_detail!(
                    "engine.fx.bridge.midi_input_overflows",
                    total
                );
            }
            Ok(())
        }
        fn midi_output_events(&mut self, index: usize) -> Result<Vec<(u32, Vec<u8>)>> {
            let output = self
                .midi_outputs
                .get(index)
                .ok_or_else(|| anyhow!("no bridge MIDI output {index}"))?;
            Ok(output[..self.midi_output_counts[index]].to_vec())
        }
        fn process(&mut self, frames: usize) -> Result<()> {
            let _process_span =
                shoop_tracing::realtime_span_detail!("engine.rt.fx.bridge_process", value = frames);
            let frames = frames.min(MAX_BLOCK_FRAMES);
            if !self.is_active() {
                for output in &mut self.audio_outputs {
                    output[..frames].fill(0.0);
                }
                self.midi_output_counts.fill(0);
                return Ok(());
            }
            self.sequence = self.sequence.wrapping_add(1).max(1);
            let token = match {
                let _submit_span = shoop_tracing::realtime_span_detail!(
                    "engine.rt.fx.bridge_submit",
                    value = self.sequence
                );
                self.transport.submit(
                    BlockSequence(self.sequence),
                    frames,
                    &self.audio_inputs,
                    self.audio_outputs.len(),
                    &self.midi_inputs,
                    &self.midi_input_counts,
                )
            } {
                Ok(token) => {
                    let _notify_span =
                        shoop_tracing::realtime_span_detail!("engine.rt.fx.bridge_notify");
                    self.wake.unpark();
                    shoop_tracing::realtime_plot_detail!(
                        "engine.fx.bridge.slot_occupancy",
                        self.transport.occupied_slots()
                    );
                    shoop_tracing::realtime_plot_detail!(
                        "engine.fx.bridge.generation",
                        self.transport.generation().0
                    );
                    token
                }
                Err(_) => {
                    let _fallback_span = shoop_tracing::realtime_span_detail!(
                        "engine.rt.fx.bridge_fallback",
                        value = 1_u64
                    );
                    for output in &mut self.audio_outputs {
                        output[..frames].fill(0.0);
                    }
                    self.midi_output_counts.fill(0);
                    let misses = self.deadline_misses.fetch_add(1, Ordering::Relaxed) + 1;
                    shoop_tracing::realtime_plot_detail!(
                        "engine.fx.bridge.deadline_misses",
                        misses
                    );
                    shoop_tracing::realtime_plot_detail!("engine.fx.bridge.fallback_reason", 1_u64);
                    return Ok(());
                }
            };
            self.shared_midi_output_count = 0;
            let completion = {
                let _wait_span = shoop_tracing::realtime_span_detail!(
                    "engine.rt.fx.bridge_wait",
                    value = self.timeout.as_nanos().min(u64::MAX as u128) as u64
                );
                self.transport.wait_and_copy_reusing_midi(
                    token,
                    Instant::now() + self.timeout,
                    &mut self.audio_outputs,
                    &mut self.shared_midi_outputs,
                    &mut self.shared_midi_output_count,
                )
            };
            if let Err(error) = completion {
                if error == crate::carla_shared_memory::SharedBlockError::MidiOverflow {
                    self.midi_output_overflows.fetch_add(1, Ordering::Relaxed);
                }
                if error == crate::carla_shared_memory::SharedBlockError::StaleCompletion {
                    self.stale_completions.fetch_add(1, Ordering::Relaxed);
                }
                let _fallback_span = shoop_tracing::realtime_span_detail!(
                    "engine.rt.fx.bridge_fallback",
                    value = 2_u64
                );
                for output in &mut self.audio_outputs {
                    output[..frames].fill(0.0);
                }
                self.midi_output_counts.fill(0);
                let misses = self.deadline_misses.fetch_add(1, Ordering::Relaxed) + 1;
                shoop_tracing::realtime_plot_detail!("engine.fx.bridge.deadline_misses", misses);
                shoop_tracing::realtime_plot_detail!("engine.fx.bridge.fallback_reason", 2_u64);
                return Ok(());
            }
            shoop_tracing::realtime_plot_detail!("engine.fx.bridge.fallback_reason", 0_u64);
            shoop_tracing::realtime_plot_detail!(
                "engine.fx.bridge.slot_occupancy",
                self.transport.occupied_slots()
            );
            self.midi_output_counts.fill(0);
            if let Some(output) = self.midi_outputs.first_mut() {
                for (slot, event) in output
                    .iter_mut()
                    .zip(self.shared_midi_outputs[..self.shared_midi_output_count].iter())
                {
                    slot.0 = event.frame_offset;
                    slot.1.clear();
                    slot.1.extend_from_slice(&event.data);
                    self.midi_output_counts[0] += 1;
                }
            }
            Ok(())
        }
    }

    fn process_bridge_block(
        transport: &mut SharedBlockTransport,
        host: &mut dyn CarlaProcessor,
        midi_inputs: &mut CarlaMidiBuffer,
        midi_outputs: &mut CarlaMidiBuffer,
    ) -> Result<bool> {
        let Some(token) = transport.worker_take() else {
            return Ok(false);
        };
        let _worker_span = shoop_tracing::realtime_span_detail!(
            "engine.rt.fx.bridge_worker",
            value = token.sequence.0
        );
        if transport.worker_audio_input_channels(token) != host.info().audio_inputs
            || transport.worker_audio_output_channels(token) != host.info().audio_outputs
        {
            return Err(anyhow!("bridge block channel layout mismatch"));
        }
        for channel in 0..host.info().audio_inputs {
            let destination = host
                .audio_input_mut(channel)
                .ok_or_else(|| anyhow!("bridge audio input disappeared"))?;
            transport.worker_copy_audio_input(token, channel, destination)?;
        }
        let (midi_pool, midi_count) = midi_inputs.storage_mut();
        transport.worker_read_midi_reusing(token, midi_pool, midi_count)?;
        if host.info().midi_inputs > 0 {
            let mut refs = [(0_u32, &[][..]); MAX_MIDI_EVENTS_PER_BLOCK];
            for (destination, event) in refs.iter_mut().zip(midi_inputs.as_slice()) {
                *destination = (event.frame_offset, event.data.as_slice());
            }
            host.set_midi_input_events(0, &refs[..midi_inputs.as_slice().len()])?;
        }
        host.process(token.frames)?;
        midi_outputs.clear();
        if host.info().midi_outputs > 0 {
            host.fill_midi_output_events(0, midi_outputs)?;
        }
        let mut audio_outputs = [&[][..]; MAX_AUDIO_CHANNELS];
        for (channel, output) in audio_outputs
            .iter_mut()
            .enumerate()
            .take(host.info().audio_outputs)
        {
            *output = host
                .audio_output(channel)
                .ok_or_else(|| anyhow!("bridge audio output disappeared"))?;
        }
        transport.worker_complete(
            token,
            &audio_outputs[..host.info().audio_outputs],
            midi_outputs.as_slice(),
        )?;
        Ok(true)
    }

    fn bridge_thread(
        mut host: Box<dyn CarlaProcessor>,
        mut transport: SharedBlockTransport,
        commands: Receiver<BridgeCommand>,
        snapshot: Arc<BridgeSnapshot>,
    ) {
        let mut midi_inputs = CarlaMidiBuffer::new(
            MAX_MIDI_EVENTS_PER_BLOCK,
            crate::midi_storage::MAX_MSG_BYTES,
        );
        let mut midi_outputs = CarlaMidiBuffer::new(
            MAX_MIDI_EVENTS_PER_BLOCK,
            crate::midi_storage::MAX_MSG_BYTES,
        );
        let mut stopped = false;
        let mut processing_faulted = false;
        while !stopped {
            while let Ok(command) = commands.try_recv() {
                match command {
                    BridgeCommand::SetActive(active) => host.set_active(active),
                    BridgeCommand::SetVisible(visible, reply) => {
                        let result = host.set_visible(visible).map_err(|e| e.to_string());
                        if result.is_ok() {
                            snapshot.visible.store(visible, Ordering::Release);
                        }
                        let _ = reply.send(result);
                    }
                    BridgeCommand::ToggleOrRecover(reply) => {
                        let result = host.toggle_or_recover().map_err(|e| e.to_string());
                        if result.is_ok() {
                            snapshot.visible.store(host.is_visible(), Ordering::Release);
                            processing_faulted = false;
                            snapshot.publish_health(host.as_mut());
                        }
                        let _ = reply.send(result);
                    }
                    BridgeCommand::SaveState(reply) => {
                        let _ = reply.send(host.save_state().map_err(|e| e.to_string()));
                    }
                    BridgeCommand::RestoreState(state, reply) => {
                        let _ = reply.send(host.restore_state(&state).map_err(|e| e.to_string()));
                    }
                    BridgeCommand::GenerationLogs(reply) => {
                        let _ = reply.send(host.generation_logs());
                    }
                    BridgeCommand::ClearLogs => host.clear_logs(),
                    BridgeCommand::Shutdown => {
                        stopped = true;
                        break;
                    }
                }
            }
            if stopped {
                break;
            }
            if processing_faulted {
                std::thread::park_timeout(Duration::from_millis(10));
                continue;
            }
            let processing = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                process_bridge_block(
                    &mut transport,
                    host.as_mut(),
                    &mut midi_inputs,
                    &mut midi_outputs,
                )
            }));
            match processing {
                Ok(Ok(true)) => snapshot.publish_health(host.as_mut()),
                Ok(Ok(false)) => {
                    snapshot.publish_health(host.as_mut());
                    std::thread::park_timeout(Duration::from_millis(10));
                }
                Ok(Err(error)) => {
                    processing_faulted = true;
                    snapshot.ready.store(false, Ordering::Release);
                    snapshot
                        .lifecycle
                        .store(CarlaProcessorLifecycle::Crashed as u8, Ordering::Release);
                    snapshot.exit_kind.store(
                        shoop_plugin_protocol::WorkerExitKind::ProtocolFailure as u8,
                        Ordering::Release,
                    );
                    snapshot
                        .crash_summary
                        .store(Some(Arc::new(error.to_string())));
                    std::thread::park_timeout(Duration::from_millis(10));
                }
                Err(_) => {
                    snapshot.ready.store(false, Ordering::Release);
                    snapshot
                        .lifecycle
                        .store(CarlaProcessorLifecycle::Crashed as u8, Ordering::Release);
                    snapshot.exit_kind.store(
                        shoop_plugin_protocol::WorkerExitKind::UnexpectedExit as u8,
                        Ordering::Release,
                    );
                    snapshot
                        .crash_summary
                        .store(Some(Arc::new("Carla processor bridge panicked".to_owned())));
                    break;
                }
            }
        }
    }

    pub fn spawn_processor_bridge(
        mut host: Box<dyn CarlaProcessor>,
        sample_rate: u32,
        nominal_buffer_size: u32,
    ) -> Result<(CarlaControlHandle, Box<dyn CarlaProcessor>)> {
        let info = host.info();
        let generation = ProcessGeneration(1);
        let nonce = *uuid::Uuid::new_v4().as_bytes();
        let mut full_nonce = [0_u8; 32];
        full_nonce[..16].copy_from_slice(&nonce);
        full_nonce[16..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
        let parent_transport = SharedBlockTransport::create(generation, &full_nonce)?;
        let worker_transport =
            SharedBlockTransport::open(parent_transport.path(), generation, &full_nonce)?;
        let snapshot = Arc::new(BridgeSnapshot::new(host.as_mut()));
        let active = Arc::clone(&snapshot.active);
        let deadline_misses = Arc::clone(&snapshot.deadline_misses);
        let midi_input_overflows = Arc::clone(&snapshot.midi_input_overflows);
        let midi_output_overflows = Arc::clone(&snapshot.midi_output_overflows);
        let stale_completions = Arc::clone(&snapshot.stale_completions);
        let (sender, receiver) = sync_channel(BRIDGE_COMMAND_CAPACITY);
        let thread_snapshot = Arc::clone(&snapshot);
        let thread = std::thread::Builder::new()
            .name("carla-processor-bridge".to_owned())
            .spawn(move || bridge_thread(host, worker_transport, receiver, thread_snapshot))?;
        let wake = thread.thread().clone();
        let control = Arc::new(BridgeControl {
            sender,
            snapshot,
            thread: Mutex::new(Some(thread)),
            wake: wake.clone(),
        });
        let handle = CarlaControlHandle {
            info,
            control: Arc::clone(&control),
        };
        let endpoint = CarlaRealtimeProcessor {
            info,
            transport: parent_transport,
            active,
            wake,
            sequence: 0,
            timeout: Duration::from_secs_f64(
                (nominal_buffer_size.max(1) as f64 / sample_rate.max(1) as f64).max(0.000_5),
            ),
            audio_inputs: vec![vec![0.0; MAX_BLOCK_FRAMES]; info.audio_inputs],
            audio_outputs: vec![vec![0.0; MAX_BLOCK_FRAMES]; info.audio_outputs],
            midi_inputs: (0..info.midi_inputs)
                .map(|_| {
                    (0..MAX_MIDI_EVENTS_PER_BLOCK)
                        .map(|_| (0, Vec::with_capacity(crate::midi_storage::MAX_MSG_BYTES)))
                        .collect()
                })
                .collect(),
            midi_input_counts: vec![0; info.midi_inputs],
            midi_outputs: (0..info.midi_outputs)
                .map(|_| {
                    (0..MAX_MIDI_EVENTS_PER_BLOCK)
                        .map(|_| (0, Vec::with_capacity(crate::midi_storage::MAX_MSG_BYTES)))
                        .collect()
                })
                .collect(),
            midi_output_counts: vec![0; info.midi_outputs],
            shared_midi_outputs: (0..MAX_MIDI_EVENTS_PER_BLOCK)
                .map(|_| MidiEvent {
                    frame_offset: 0,
                    data: Vec::with_capacity(crate::midi_storage::MAX_MSG_BYTES),
                })
                .collect(),
            shared_midi_output_count: 0,
            deadline_misses,
            midi_input_overflows,
            midi_output_overflows,
            stale_completions,
        };
        Ok((handle, Box::new(endpoint)))
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use bridge::{spawn_processor_bridge, CarlaControlHandle, CarlaRealtimeProcessor};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FakeProcessorBehavior {
    pub process_delay: Duration,
    pub fail_processing: bool,
    pub panic_processing: bool,
    pub fail_state: bool,
    pub fail_visibility: bool,
}

#[derive(Debug)]
pub struct FakeCarlaProcessor {
    info: CarlaProcessorInfo,
    audio_inputs: Vec<Vec<f32>>,
    audio_outputs: Vec<Vec<f32>>,
    midi_inputs: Vec<Vec<(u32, Vec<u8>)>>,
    midi_outputs: Vec<Vec<(u32, Vec<u8>)>>,
    active: bool,
    visible: bool,
    state: String,
    behavior: FakeProcessorBehavior,
}

impl FakeCarlaProcessor {
    pub fn new(chain_type: FXChainType, audio_channels: usize, max_frames: usize) -> Self {
        Self {
            info: CarlaProcessorInfo {
                chain_type,
                audio_inputs: audio_channels,
                audio_outputs: audio_channels,
                midi_inputs: 1,
                midi_outputs: 1,
            },
            audio_inputs: vec![vec![0.0; max_frames]; audio_channels],
            audio_outputs: vec![vec![0.0; max_frames]; audio_channels],
            midi_inputs: vec![Vec::new()],
            midi_outputs: vec![Vec::new()],
            active: false,
            visible: false,
            state: "{}".to_owned(),
            behavior: FakeProcessorBehavior::default(),
        }
    }

    pub fn set_behavior(&mut self, behavior: FakeProcessorBehavior) {
        self.behavior = behavior;
    }
}

impl CarlaProcessor for FakeCarlaProcessor {
    fn info(&self) -> CarlaProcessorInfo {
        self.info
    }

    fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn set_visible(&mut self, visible: bool) -> Result<()> {
        if self.behavior.fail_visibility {
            anyhow::bail!("fake visibility failure");
        }
        self.visible = visible;
        Ok(())
    }

    fn is_visible(&mut self) -> bool {
        self.visible
    }

    fn save_state(&mut self) -> Result<String> {
        if self.behavior.fail_state {
            anyhow::bail!("fake state save failure");
        }
        Ok(self.state.clone())
    }

    fn restore_state(&mut self, state: &str) -> Result<()> {
        if self.behavior.fail_state {
            anyhow::bail!("fake state restore failure");
        }
        self.state = state.to_owned();
        Ok(())
    }

    fn audio_input_mut(&mut self, index: usize) -> Option<&mut [f32]> {
        self.audio_inputs.get_mut(index).map(Vec::as_mut_slice)
    }

    fn audio_output(&self, index: usize) -> Option<&[f32]> {
        self.audio_outputs.get(index).map(Vec::as_slice)
    }

    fn set_midi_input_events(&mut self, index: usize, events: &[(u32, &[u8])]) -> Result<()> {
        let destination = self
            .midi_inputs
            .get_mut(index)
            .ok_or_else(|| anyhow::anyhow!("no fake MIDI input {index}"))?;
        destination.clear();
        destination.extend(events.iter().map(|(offset, data)| (*offset, data.to_vec())));
        Ok(())
    }

    fn midi_output_events(&mut self, index: usize) -> Result<Vec<(u32, Vec<u8>)>> {
        self.midi_outputs
            .get(index)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no fake MIDI output {index}"))
    }

    fn fill_midi_output_events(
        &mut self,
        index: usize,
        destination: &mut CarlaMidiBuffer,
    ) -> Result<()> {
        destination.clear();
        for (frame_offset, data) in self
            .midi_outputs
            .get(index)
            .ok_or_else(|| anyhow::anyhow!("no fake MIDI output {index}"))?
        {
            destination.push(*frame_offset, data)?;
        }
        Ok(())
    }

    fn process(&mut self, frames: usize) -> Result<()> {
        if self.behavior.panic_processing {
            panic!("fake processor panic");
        }
        if !self.behavior.process_delay.is_zero() {
            std::thread::sleep(self.behavior.process_delay);
        }
        if self.behavior.fail_processing {
            anyhow::bail!("fake processing failure");
        }
        if !self.active {
            return Ok(());
        }
        for (input, output) in self.audio_inputs.iter().zip(&mut self.audio_outputs) {
            let frames = frames.min(input.len()).min(output.len());
            output[..frames].copy_from_slice(&input[..frames]);
        }
        for (input, output) in self.midi_inputs.iter().zip(&mut self.midi_outputs) {
            output.clone_from(input);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(target_arch = "wasm32"))]
    use shoop_plugin_protocol::MAX_BLOCK_FRAMES;

    #[test]
    fn fake_processor_round_trips_audio_midi_state_and_visibility() {
        let mut processor = FakeCarlaProcessor::new(FXChainType::CarlaRack, 2, 64);
        processor.set_active(true);
        processor.set_visible(true).unwrap();
        processor.restore_state("checkpoint").unwrap();
        processor.audio_input_mut(0).unwrap()[..4].copy_from_slice(&[1.0, 2.0, 3.0, 4.0]);
        processor
            .set_midi_input_events(0, &[(3, &[0x90, 60, 100])])
            .unwrap();
        processor.process(4).unwrap();

        assert_eq!(
            processor.audio_output(0).unwrap()[..4],
            [1.0, 2.0, 3.0, 4.0]
        );
        assert_eq!(
            processor.midi_output_events(0).unwrap(),
            vec![(3, vec![0x90, 60, 100])]
        );
        assert_eq!(processor.save_state().unwrap(), "checkpoint");
        assert!(processor.is_visible());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn bridge_realtime_endpoint_is_lock_free_allocation_free_and_bounded() {
        let fake = FakeCarlaProcessor::new(FXChainType::CarlaRack, 2, MAX_BLOCK_FRAMES);
        let (control, mut endpoint) = spawn_processor_bridge(Box::new(fake), 1_000, 100).unwrap();
        control.set_active(true);
        endpoint.audio_input_mut(0).unwrap()[..4].copy_from_slice(&[1.0, 2.0, 3.0, 4.0]);
        endpoint
            .set_midi_input_events(0, &[(2, &[0x90, 64, 100]), (3, &[0xf0, 1, 2, 3, 0xf7])])
            .unwrap();
        assert_eq!(control.midi_input_overflows(), 1);
        endpoint.process(4).unwrap();
        assert_eq!(endpoint.audio_output(0).unwrap()[..4], [1.0, 2.0, 3.0, 4.0]);
        assert_eq!(
            endpoint.midi_output_events(0).unwrap(),
            vec![(2, vec![0x90, 64, 100])]
        );
        assert_no_alloc::assert_no_alloc(|| endpoint.process(4).unwrap());
        assert!(endpoint.audio_output(0).unwrap()[..4]
            .iter()
            .all(|sample| sample.is_finite()));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn bridge_deadline_miss_returns_wet_silence() {
        let mut fake = FakeCarlaProcessor::new(FXChainType::CarlaRack, 2, MAX_BLOCK_FRAMES);
        fake.set_behavior(FakeProcessorBehavior {
            process_delay: Duration::from_millis(20),
            ..Default::default()
        });
        let (control, mut endpoint) = spawn_processor_bridge(Box::new(fake), 48_000, 32).unwrap();
        control.set_active(true);
        endpoint.audio_input_mut(0).unwrap()[..32].fill(1.0);
        let started = std::time::Instant::now();
        endpoint.process(32).unwrap();
        assert!(started.elapsed() < Duration::from_millis(15));
        assert!(endpoint.audio_output(0).unwrap()[..32]
            .iter()
            .all(|sample| *sample == 0.0));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn bridge_contains_processor_panics_and_publishes_failure() {
        let mut fake = FakeCarlaProcessor::new(FXChainType::CarlaRack, 2, MAX_BLOCK_FRAMES);
        fake.set_behavior(FakeProcessorBehavior {
            panic_processing: true,
            ..Default::default()
        });
        let (control, mut endpoint) = spawn_processor_bridge(Box::new(fake), 48_000, 32).unwrap();
        control.set_active(true);
        endpoint.process(32).unwrap();
        assert_eq!(control.lifecycle(), CarlaProcessorLifecycle::Crashed);
        assert_eq!(
            control.exit_kind(),
            shoop_plugin_protocol::WorkerExitKind::UnexpectedExit
        );
        assert!(control
            .crash_summary()
            .is_some_and(|summary| summary.contains("panicked")));
        assert!(endpoint.audio_output(0).unwrap()[..32]
            .iter()
            .all(|sample| *sample == 0.0));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn bridge_shutdown_soak_reclaims_threads_and_mappings() {
        for iteration in 0..100 {
            let fake = FakeCarlaProcessor::new(FXChainType::CarlaRack, 2, MAX_BLOCK_FRAMES);
            let (control, mut endpoint) =
                spawn_processor_bridge(Box::new(fake), 1_000, 100).unwrap();
            control.set_active(true);
            endpoint.process(16).unwrap();
            if iteration % 2 == 0 {
                drop(endpoint);
                drop(control);
            } else {
                drop(control);
                drop(endpoint);
            }
        }
    }

    #[test]
    fn fake_processor_exposes_failures_and_delay() {
        let mut processor = FakeCarlaProcessor::new(FXChainType::CarlaRack, 2, 64);
        processor.set_behavior(FakeProcessorBehavior {
            process_delay: Duration::from_millis(1),
            fail_processing: true,
            fail_state: true,
            fail_visibility: true,
            ..Default::default()
        });
        let started = std::time::Instant::now();
        assert!(processor.process(4).is_err());
        assert!(started.elapsed() >= Duration::from_millis(1));
        assert!(processor.save_state().is_err());
        assert!(processor.restore_state("state").is_err());
        assert!(processor.set_visible(true).is_err());
    }
}
