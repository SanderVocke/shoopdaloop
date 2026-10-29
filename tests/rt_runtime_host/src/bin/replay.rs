use rt_runtime::transport::{channel, Lane, Outcome, TransportConfig};
use rt_runtime::{Epoch, Frame};
use rt_runtime_host_fixture::{guarded, wire::*, Bridge, Controller, QUANTUM};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[derive(Clone, Copy)]
struct Batch {
    input: u32,
    packets: [Packet; SLOTS],
    output: u32,
    replies: [Packet; SLOTS],
    hash: u32,
}
struct Reader {
    bytes: Vec<u8>,
    offset: usize,
}
impl Reader {
    fn bytes(&mut self, count: usize) -> &[u8] {
        let end = self.offset.checked_add(count).expect("journal range");
        let bytes = self.bytes.get(self.offset..end).expect("complete journal");
        self.offset = end;
        bytes
    }
    fn u32(&mut self) -> u32 {
        u32::from_le_bytes(self.bytes(4).try_into().unwrap())
    }
    fn packet(&mut self) -> Packet {
        let len = self.u32() as usize;
        assert!((HEADER..=BYTES).contains(&len));
        let mut packet = Packet::default();
        packet.len = len;
        packet.bytes[..len].copy_from_slice(self.bytes(len));
        packet
    }
}
fn load(path: &str) -> Vec<Batch> {
    assert!(std::fs::metadata(path).unwrap().len() <= 64 * 1024 * 1024);
    let mut reader = Reader {
        bytes: std::fs::read(path).unwrap(),
        offset: 0,
    };
    assert_eq!(reader.bytes(4), b"RTJ1");
    let count = reader.u32() as usize;
    assert!(count > 0 && count <= 4096);
    let mut batches = Vec::with_capacity(count);
    for _ in 0..count {
        let mut batch = Batch {
            input: reader.u32(),
            packets: [Packet::default(); SLOTS],
            output: 0,
            replies: [Packet::default(); SLOTS],
            hash: 0,
        };
        assert_eq!(batch.input & !31, 0);
        for slot in 0..SLOTS {
            if batch.input & (1 << slot) != 0 {
                batch.packets[slot] = reader.packet();
            }
        }
        batch.output = reader.u32();
        assert_eq!(batch.output & !31, 0);
        for slot in 0..SLOTS {
            if batch.output & (1 << slot) != 0 {
                batch.replies[slot] = reader.packet();
            }
        }
        batch.hash = reader.u32();
        batches.push(batch);
    }
    assert_eq!(reader.offset, reader.bytes.len());
    batches
}
fn process(bridge: &mut Bridge, batch: &Batch) {
    guarded(|| {
        for slot in 0..SLOTS {
            if batch.input & (1 << slot) != 0 {
                bridge.load(slot, batch.packets[slot]).unwrap();
            }
        }
        if batch.input == 31 {
            assert_eq!(
                bridge.load(0, batch.packets[0]),
                Err(rt_runtime::Error::Busy)
            );
        }
        let ready = bridge.render();
        assert_eq!(
            ready,
            batch.output,
            "ready mask at frame {}",
            bridge.next_frame()
        );
        for slot in 0..SLOTS {
            if ready & (1 << slot) != 0 {
                let actual = bridge.packet(slot);
                let expected = batch.replies[slot];
                assert_eq!(actual.len, expected.len);
                assert_eq!(
                    &actual.bytes[..actual.len],
                    &expected.bytes[..expected.len],
                    "response at frame {} slot {slot}",
                    bridge.next_frame()
                );
            }
        }
        let hash = bridge.output().iter().fold(2166136261u32, |hash, sample| {
            (hash ^ sample.to_bits()).wrapping_mul(16777619)
        });
        assert_eq!(
            hash,
            batch.hash,
            "rendered sample hash at frame {}",
            bridge.next_frame()
        );
    });
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert!(args.len() >= 2, "replay JOURNAL [--threaded]");
    let batches = load(&args[1]);
    let threaded = args.iter().any(|arg| arg == "--threaded");
    let count = batches.len();
    let mut controller = Controller::new();
    let mut graph_hash = 2166136261u32;
    for variant in 0..3 {
        controller.compile_graph(variant);
        for byte in controller.descriptor() {
            graph_hash = (graph_hash ^ u32::from(*byte)).wrapping_mul(16777619);
        }
    }
    let bridge = if threaded {
        let (mut control, mut render) = channel::<Batch, (), ()>(TransportConfig {
            normal_slots: 2,
            recovery_slots: 1,
            telemetry_slots: 1,
            operations_per_cycle: 1,
            recovery_per_cycle: 1,
            publications_per_cycle: 1,
        })
        .unwrap();
        control.prepare_submissions().unwrap();
        let available = Arc::new(AtomicUsize::new(0));
        let ready = Arc::clone(&available);
        let thread = std::thread::spawn(move || {
            let mut bridge = Bridge::new(1);
            for index in 0..count {
                // Offline host clock coordination, outside the nonblocking callback.
                while ready.load(Ordering::Acquire) <= index {
                    std::thread::yield_now();
                }
                guarded(|| {
                    render.begin_cycle(Frame(index as u64 * QUANTUM as u64), Epoch(0));
                    let batch = *render
                        .next_operation()
                        .expect("announced host quantum")
                        .request;
                    process(&mut bridge, &batch);
                    render.complete(Outcome::Applied, None).unwrap();
                });
            }
            bridge
        });
        let mut sent = 0;
        let mut received = 0;
        while received < count {
            while sent < count && control.outstanding() < 2 {
                control
                    .submit_prepared(Lane::Normal, batches[sent])
                    .map_err(|e| e.error)
                    .unwrap();
                sent += 1;
                available.store(sent, Ordering::Release);
            }
            if let Some(completion) = control.try_completion() {
                assert_eq!(completion.outcome, Outcome::Applied);
                received += 1;
            } else {
                assert!(control.connected(), "render host disconnected");
                std::thread::yield_now();
            }
        }
        thread.join().unwrap()
    } else {
        let mut bridge = Bridge::new(1);
        for batch in &batches {
            process(&mut bridge, batch);
        }
        bridge
    };
    assert_eq!(bridge.next_frame(), count as u64 * QUANTUM as u64);
    println!("{{\"mode\":\"{}\",\"quanta\":{},\"frames\":{},\"sample_hash\":{},\"midi_hash\":{},\"midi_events\":{},\"graph_hash\":{},\"max_copy\":{},\"max_steps\":{}}}",
        if threaded { "two-thread" } else { "offline" }, count, bridge.next_frame(), bridge.sample_hash(), bridge.midi_hash(), bridge.midi_events(), graph_hash, bridge.max_copy(), bridge.max_steps());
}
