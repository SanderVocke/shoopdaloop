//! Offline drivers. Thread coordination and allocating reports stay outside callbacks.
use crate::{
    wire::{Phase, Response, Status},
    Bridge, Client, Report,
};
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    Frame,
};
#[cfg(not(target_arch = "wasm32"))]
use shoop_loop_engine::LoopId;
use shoop_loop_engine::Transition;

const SESSION: u64 = 0x1234_5678_9abc_def0;
fn script() -> [(u64, Transition); 3] {
    [
        (
            0,
            Transition::Record {
                monitor_previous: false,
            },
        ),
        (
            4,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
        ),
        (12, Transition::Stop),
    ]
}
fn validate_partitions(partitions: &[usize]) {
    assert!(partitions.iter().all(|&frames| frames <= 128));
    assert_eq!(partitions.iter().sum::<usize>(), 13);
}
struct Buffers {
    input_left: [f32; 128],
    input_right: [f32; 128],
    left: [f32; 128],
    right: [f32; 128],
    input: [MidiBuffer; 1],
    output: [MidiBuffer; 1],
}
impl Buffers {
    fn new() -> Self {
        Self {
            input_left: [0.; 128],
            input_right: [0.; 128],
            left: [0.; 128],
            right: [0.; 128],
            input: [MidiBuffer::new(2, 6).unwrap()],
            output: [MidiBuffer::new(256, 1024).unwrap()],
        }
    }
    fn render(&mut self, bridge: &mut Bridge, frames: usize, report: &mut Accumulator) {
        let start = bridge.next_frame();
        self.input_left.fill(0.);
        self.input_right.fill(0.);
        self.left.fill(0.);
        self.right.fill(0.);
        self.input[0].clear();
        self.output[0].clear();
        for offset in 0..frames {
            if start + (offset as u64) < 4 {
                self.input_left[offset] = 2. * (start + offset as u64 + 1) as f32;
                self.input_right[offset] = self.input_left[offset] * 10.;
            }
        }
        for (at, packet) in [(0, [0x90, 60, 100]), (2, [0x80, 60, 45])] {
            if at >= start && at < start + frames as u64 {
                self.input[0]
                    .push((at - start) as u32, frames, &packet)
                    .unwrap();
            }
        }
        bridge
            .process(
                AudioBlock {
                    frame: Frame(start),
                    frames,
                    inputs: &[&self.input_left[..frames], &self.input_right[..frames]],
                    outputs: &mut [&mut self.left[..frames], &mut self.right[..frames]],
                },
                MidiBlock {
                    inputs: &self.input,
                    outputs: &mut self.output,
                },
            )
            .expect("guarded bridge rendering");
        assert_eq!(bridge.next_frame(), start + frames as u64);
        for offset in 0..frames {
            report.samples.push([self.left[offset], self.right[offset]]);
        }
        for index in 0..self.output[0].len() {
            let (offset, packet) = self.output[0].get(index).unwrap();
            report
                .midi
                .push((start + u64::from(offset), packet.to_vec()));
        }
    }
}
#[derive(Default)]
struct Accumulator {
    samples: Vec<[f32; 2]>,
    midi: Vec<(u64, Vec<u8>)>,
    transitions: Vec<(u64, u64, u64)>,
    admissions: usize,
}
impl Accumulator {
    fn receipt(&mut self, response: Response) {
        assert_eq!(response.error, 0);
        assert_eq!(response.effective, response.requested);
        match response.phase {
            Phase::Admission => {
                assert_eq!(response.status, Status::Admitted);
                assert_eq!(response.observed, 0);
                self.admissions += 1;
            }
            Phase::Execution => {
                assert_eq!(response.status, Status::Applied);
                self.transitions.push((
                    response.requested,
                    response.delivered.unwrap(),
                    response.observed,
                ));
            }
        }
    }
    fn finish(self, bridge: Bridge) -> Report {
        assert_eq!(self.admissions, 3);
        assert_eq!(self.transitions, [(0, 0, 0), (4, 4, 4), (12, 12, 12)]);
        assert_eq!(bridge.outstanding(), 0);
        let mut stopped = bridge.into_stopped();
        stopped.cleanup().unwrap();
        for _ in 0..512 {
            stopped.resources().begin_callback();
            stopped.resources().reclaim();
        }
        let resources = stopped.resource_usage();
        let media = stopped.media_usage();
        assert_eq!(
            (
                resources.free_pages,
                resources.free_versions,
                resources.free_snapshots,
                media.references
            ),
            (256, 16, 16, 0)
        );
        Report {
            samples: self.samples,
            midi: self.midi,
            transitions: self.transitions,
            free_pages: resources.free_pages,
            free_versions: resources.free_versions,
            free_snapshots: resources.free_snapshots,
            media_references: media.references,
        }
    }
}

pub fn run_wire(partitions: &[usize]) -> Report {
    validate_partitions(partitions);
    let mut bridge = Bridge::new(SESSION).unwrap();
    let mut client = Client::new(SESSION).unwrap();
    for (at, transition) in script() {
        let packet =
            crate::guarded(|| client.prepare(at, bridge.target(), transition, false, false))
                .unwrap();
        bridge.ingest(&packet).unwrap();
    }
    let mut buffers = Buffers::new();
    let mut report = Accumulator::default();
    for &frames in partitions {
        buffers.render(&mut bridge, frames, &mut report);
        while let Some(packet) = bridge.next_response().unwrap() {
            report.receipt(crate::guarded(|| client.receive(&packet)).unwrap());
        }
    }
    assert_eq!(client.outstanding(), 0);
    report.finish(bridge)
}

/// Bounded two-thread offline replay. Blocking channels and join are deliberately
/// outside guarded callbacks; this is not a live device scheduling benchmark.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_two_thread(partitions: &[usize]) -> Report {
    validate_partitions(partitions);
    let partitions = partitions.to_vec();
    let (requests, ingress) = std::sync::mpsc::sync_channel::<[u8; crate::wire::REQUEST_BYTES]>(1);
    let (returns, responses) =
        std::sync::mpsc::sync_channel::<[u8; crate::wire::RESPONSE_BYTES]>(1);
    let worker = std::thread::spawn(move || {
        let mut bridge = Bridge::new(SESSION).unwrap();
        let mut buffers = Buffers::new();
        let mut report = Accumulator::default();
        for _ in script() {
            bridge
                .ingest(&ingress.recv().expect("complete native script"))
                .unwrap();
        }
        for frames in partitions {
            buffers.render(&mut bridge, frames, &mut report);
            while let Some(packet) = bridge.next_response().unwrap() {
                report.receipt(Response::decode(&packet).unwrap());
                returns.send(packet).expect("native control receiver");
            }
        }
        report.finish(bridge)
    });
    let mut client = Client::new(SESSION).unwrap();
    for (at, transition) in script() {
        let packet = crate::guarded(|| {
            client.prepare(
                at,
                LoopId::from_parts(0, 1).unwrap(),
                transition,
                false,
                false,
            )
        })
        .unwrap();
        requests.send(packet).expect("native render receiver");
    }
    drop(requests);
    for packet in responses {
        crate::guarded(|| client.receive(&packet)).unwrap();
    }
    assert_eq!(client.outstanding(), 0);
    worker.join().expect("native render worker")
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    #[test]
    fn bounded_two_thread_and_wire_drivers_match_the_independent_native_oracle() {
        let expected = crate::run(&[13]);
        for partitions in [
            vec![13],
            vec![1; 13],
            vec![3, 5, 2, 3],
            vec![0, 4, 0, 8, 0, 1, 0],
        ] {
            assert_eq!(super::run_wire(&partitions), expected);
            assert_eq!(super::run_two_thread(&partitions), expected);
        }
    }
}
