use std::time::Instant;

use super::Cycle;
use crate::status::Shared;

pub(crate) trait DirectIo {
    fn begin(&mut self);
    fn input(&self, port: usize, offset: usize, samples: &mut [f32]);
    fn output(&mut self, port: usize, offset: usize, samples: &[f32]);
    fn midi_input(&self, port: usize, receive: &mut dyn FnMut(u32, &[u8]));
    fn midi_output(&mut self, port: usize, offset: u32, bytes: &[u8]) -> bool;
}

impl Cycle {
    pub(crate) fn process_direct(&mut self, frames: usize, io: &mut impl DirectIo) {
        io.begin();
        if frames == 0 {
            return;
        }
        let started = Instant::now();
        self.shared.begin_profiling(self.next_frame(), frames);
        Shared::count(&self.shared.counters.callbacks, 1);
        self.shared.set_buffer_frames(frames as u32);
        if frames > self.max_frames {
            Shared::count(&self.shared.counters.split_callbacks, 1);
        }
        for base in (0..frames).step_by(self.max_frames) {
            let count = (frames - base).min(self.max_frames);
            for (port, input) in self.inputs.iter_mut().enumerate() {
                io.input(port, base, &mut input[..count]);
            }
            for (port, input) in self.midi_inputs.iter_mut().enumerate() {
                let counters = &self.shared.counters;
                let lost = &mut self.input_lost[port];
                let already_lost = *lost;
                io.midi_input(port, &mut |offset, bytes| {
                    let offset = offset as usize;
                    if offset >= base && offset < base + count {
                        if *lost {
                            Shared::count(&counters.midi_in_dropped, 1);
                            return;
                        }
                        if let Err(error) = input.push((offset - base) as u32, count, bytes) {
                            let counter = if error == rt_runtime::Error::Full {
                                *lost = true;
                                &counters.midi_in_dropped
                            } else {
                                &counters.midi_in_refused
                            };
                            Shared::count(counter, 1);
                        }
                    } else if base == 0 && offset >= frames {
                        Shared::count(&counters.midi_in_refused, 1);
                    }
                });
                if *lost {
                    if already_lost {
                        input.mark_input_loss_silent();
                    } else {
                        input.mark_input_loss();
                    }
                }
            }
            self.render(count, 0, false);
            for (port, output) in self.outputs.iter().enumerate() {
                io.output(port, base, &output[..count]);
            }
            for (port, output) in self.midi_outputs.iter().enumerate() {
                let reset = &mut self.output_reset[port];
                if output.is_recovery()
                    || self
                        .render
                        .midi_recovery(port)
                        .is_some_and(|b| !b.is_empty())
                {
                    reset.request();
                }
                if !reset.pending() {
                    for index in 0..output.len() {
                        let (offset, bytes) = output.get(index).expect("bounded output");
                        if io.midi_output(port, base as u32 + offset, bytes) {
                            continue;
                        }
                        Shared::count(&self.shared.counters.midi_out_failed, 1);
                        Shared::count(
                            &self.shared.counters.midi_out_dropped,
                            (output.len() - index) as u64,
                        );
                        reset.request();
                        break;
                    }
                }
                while let Some(packet) = reset.peek() {
                    if !io.midi_output(port, base as u32, &packet) {
                        Shared::count(&self.shared.counters.midi_out_failed, 1);
                        break;
                    }
                    reset.acknowledge().expect("accepted reset");
                }
            }
        }
        self.shared.set_load(
            started.elapsed().as_secs_f32() * self.shared.sample_rate() as f32 / frames as f32,
        );
        self.shared
            .callback_finished
            .fetch_add(1, std::sync::atomic::Ordering::Release);
        if self.phase == shoop_rt_engine::Phase::Disconnected {
            self.shared.reset_flushed.store(
                self.output_reset.iter().all(|reset| !reset.pending()),
                std::sync::atomic::Ordering::Release,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NativeClient, RenderPhase};
    use shoop_rt_engine::{prepare, EngineDescription, HostPorts, Segment};
    use shoop_rt_protocol::*;
    use shoop_rt_transport::{carrier::InProcess, client::ProtocolClient};
    use shoop_wasm_test_support::{
        heap::{assert_no_heap, measure, TrackingAllocator},
        shoop_test,
    };
    use std::sync::Arc;

    #[global_allocator]
    static ALLOCATOR: TrackingAllocator = TrackingAllocator;

    struct Io {
        events: Vec<(u32, [u8; 3])>,
        // Fixed payload storage keeps the mock writer allocation-free too.
        packets: [(u32, [u8; 3]); 128],
        len: usize,
        audio: [f32; 128],
        reject: bool,
    }
    impl Io {
        fn new() -> Self {
            Self {
                events: Vec::new(),
                packets: [(0, [0; 3]); 128],
                len: 0,
                audio: [0.; 128],
                reject: false,
            }
        }
        fn messages(&mut self) -> &[(u32, [u8; 3])] {
            &self.packets[..self.len]
        }
    }
    impl DirectIo for Io {
        fn begin(&mut self) {
            self.len = 0;
            self.audio.fill(0.);
        }
        fn input(&self, _: usize, offset: usize, samples: &mut [f32]) {
            for (index, sample) in samples.iter_mut().enumerate() {
                *sample = (offset + index + 1) as f32 / 128.;
            }
        }
        fn output(&mut self, _: usize, offset: usize, samples: &[f32]) {
            self.audio[offset..offset + samples.len()].copy_from_slice(samples);
        }
        fn midi_input(&self, _: usize, receive: &mut dyn FnMut(u32, &[u8])) {
            for (offset, bytes) in &self.events {
                receive(*offset, bytes);
            }
        }
        fn midi_output(&mut self, _: usize, offset: u32, bytes: &[u8]) -> bool {
            if self.reject {
                return false;
            }
            assert_eq!(bytes.len(), 3);
            self.packets[self.len] = (offset, bytes.try_into().unwrap());
            self.len += 1;
            true
        }
    }
    fn setup(events: usize) -> (Cycle, NativeClient, Io) {
        let description = EngineDescription {
            quantum: 1,
            sample_rate: 48000,
            max_frames: 8,
            host: HostPorts::numbered(1, 1, 1),
            segments: vec![Segment {
                strips: 2,
                loops: 2,
            }],
            strip_channels: 1,
            master_channels: 1,
            master_connections: Vec::new(),
            resource_pages: 64,
            page_bytes: 256,
            midi_events: events,
            midi_bytes: 512,
            smoothing_frames: 0,
            smoothing_limit_frames: 8,
            ramp_frames: 4,
            control_lead_frames: 0,
            ..EngineDescription::default()
        };
        let driver = DriverStatus {
            kind: DriverKind::Jack,
            state: DriverRunState::Running,
            sample_rate: 48000,
            buffer_frames: 8,
        };
        let engine = prepare(&description, driver).unwrap();
        let outbox = engine.outbox();
        let shared = Arc::new(Shared::new(engine.host, driver));
        let mut client = ProtocolClient::new(InProcess::new(engine.endpoint, outbox));
        let mut cycle =
            Cycle::new(engine.render, shared, &description, Vec::new(), Vec::new()).unwrap();
        let mut io = Io::new();
        client.connect().unwrap();
        while client.poll().is_some() {}
        let strip = StripId(SlotId::new(0, 1));
        let mut requests = vec![Request::CreateStrip {
            strip,
            shape: StripShape {
                audio_inputs: 1,
                audio_outputs: 1,
                midi_input: true,
                midi_output: true,
            },
        }];
        for (kind, index) in [
            (PortKind::AudioIn, 0),
            (PortKind::AudioOut, 1),
            (PortKind::MidiIn, 2),
            (PortKind::MidiOut, 3),
        ] {
            requests.push(Request::Connect {
                port: EnginePort::Strip(StripPort {
                    strip,
                    kind,
                    index: 0,
                }),
                host: HostPortId(index),
                connected: true,
            });
        }
        requests.push(Request::StripControl {
            strip,
            control: StripControl::Monitoring(true),
        });
        for request in requests {
            let id = client.request(request).unwrap();
            let mut done = false;
            for _ in 0..64 {
                assert_no_heap(|| cycle.process_direct(8, &mut io));
                while let Some(message) = client.poll() {
                    if let EngineMessage::Result {
                        correlation,
                        outcome,
                    } = message
                    {
                        if correlation == id {
                            assert_eq!(outcome, Outcome::Done);
                            done = true;
                        }
                    }
                }
                if done {
                    break;
                }
            }
            assert!(done, "request timed out: {request:?}");
        }
        (cycle, client, io)
    }
    #[shoop_test(no_wasm = "native callback adapter")]
    fn direct_audio_and_midi_keep_offsets_across_split_callbacks_without_heap_work() {
        let (mut cycle, client, mut io) = setup(64);
        io.events = [0, 0, 7, 8, 15, 16]
            .into_iter()
            .enumerate()
            .map(|(index, offset)| (offset, [0xb0, 1, index as u8]))
            .collect();
        assert_no_heap(|| cycle.process_direct(17, &mut io));
        let expected = io.events.clone();
        assert_eq!(io.messages(), expected);
        for (index, sample) in io.audio[..17].iter().enumerate() {
            assert_eq!(*sample, (index + 1) as f32 / 128.);
        }
        assert_eq!(cycle.shared.stats().split_callbacks, 1);
        let callbacks = cycle.shared.stats().callbacks;
        assert_no_heap(|| cycle.process_direct(0, &mut io));
        assert_eq!(cycle.shared.stats().callbacks, callbacks);
        io.events.clear();
        drop(client);
        assert_no_heap(|| cycle.process_direct(17, &mut io));
        assert_eq!(cycle.shared.phase(), RenderPhase::Disconnected);
        assert_eq!(io.messages().len(), 32);
        assert!(io
            .messages()
            .iter()
            .all(|(offset, bytes)| *offset == 0 && (bytes[1] == 120 || bytes[1] == 123)));
        assert!(io.audio[..17].iter().all(|sample| *sample == 0.));
    }
    #[shoop_test(no_wasm = "native callback adapter")]
    fn direct_midi_capacity_and_writer_failures_are_counted() {
        let (mut cycle, _client, mut io) = setup(2);
        io.events = vec![
            (0, [0xb0, 1, 1]),
            (1, [0xb0, 1, 2]),
            (2, [0xb0, 1, 3]),
            (9, [0xb0, 1, 4]),
        ];
        io.reject = true;
        assert_no_heap(|| cycle.process_direct(8, &mut io));
        let stats = cycle.shared.stats();
        assert_eq!(stats.midi_in_dropped, 1);
        assert_eq!(stats.midi_in_refused, 1);
        assert_eq!(stats.midi_out_failed, 1);
        assert!(cycle.output_reset[0].pending());
        io.reject = false;
        io.events.clear();
        assert_no_heap(|| cycle.process_direct(8, &mut io));
        assert!(!cycle.output_reset[0].pending());
        assert_eq!(io.messages().len(), 32);
        assert_eq!(io.messages()[0].1, [0xb0, 120, 0]);
    }
    #[shoop_test(no_wasm = "native allocator guard")]
    fn direct_callback_heap_guard_detects_allocations_and_destruction() {
        let (_, counts) = measure(|| drop(std::hint::black_box(vec![0u8; 16])));
        assert_eq!(counts.allocations, 1);
        assert_eq!(counts.deallocations, 1);
    }
}
