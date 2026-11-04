mod support;
use shoop_rt_protocol::*;
use shoop_wasm_test_support::shoop_test;
use support::*;
#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn upload(h: &mut Harness, id: u64, frames: usize) {
    let bytes: Vec<u8> = (0..frames)
        .flat_map(|_| [0.25f32, 0.0].into_iter().flat_map(f32::to_le_bytes))
        .collect();
    h.done(Request::ContentBegin(ContentDescription {
        id,
        midi: false,
        frames: frames as u64,
        bytes: bytes.len() as u64,
    }));
    for (index, chunk) in bytes.chunks(128).enumerate() {
        h.done(Request::ContentWrite {
            id,
            offset: (index * 128) as u64,
            data: ContentChunk::new(chunk).unwrap(),
        });
    }
    h.done(Request::ContentFinish { id });
}
fn start(h: &mut Harness, id: u64, frames: u64) -> u64 {
    h.send(Request::PreviewStart {
        request_id: id,
        bus: BusId(SlotId::new(0, 1)),
        content: id,
        frames,
    })
}
#[shoop_test]
fn preview_reaches_bus_outputs_respects_controls_and_releases_without_heap_work() {
    let mut h = Harness::new(&description());
    upload(&mut h, 1, 512);
    assert_eq!(
        h.apply(Request::PreviewStart {
            request_id: 1,
            bus: BusId::MASTER,
            content: 1,
            frames: 512
        }),
        Outcome::Refused(Refusal::Busy)
    );
    h.done(Request::CreateStrip {
        strip: strip(0, 1),
        shape: shape(1, 1, false),
    });
    let before = h.outputs[0].len();
    let operation = start(&mut h, 1, 512);
    h.run(64, 16);
    assert!(!h.results.contains_key(&operation));
    assert!(h.outputs[0][before..].contains(&0.25));
    assert_eq!(h.outputs[0][before..], h.outputs[1][before..]);
    assert_eq!(
        h.apply(Request::ContentRelease { id: 1 }),
        Outcome::Refused(Refusal::Busy)
    );
    h.done(Request::BusControl {
        bus: BusId(SlotId::new(0, 1)),
        control: BusControl::Gain(0.5),
    });
    h.run(32, 16);
    assert_eq!(*h.outputs[0].last().unwrap(), 0.125);
    h.done(Request::BusControl {
        bus: BusId(SlotId::new(0, 1)),
        control: BusControl::Mute(true),
    });
    h.run(32, 16);
    assert_eq!(*h.outputs[0].last().unwrap(), 0.0);
    h.done(Request::PreviewStop);
    h.run(64, 16);
    assert_eq!(h.results[&operation], Outcome::Done);
    h.done(Request::ContentRelease { id: 1 });
    h.done(Request::BusControl {
        bus: BusId(SlotId::new(0, 1)),
        control: BusControl::Mute(false),
    });
    let after = h.outputs[0].len();
    h.run(64, 16);
    assert!(h.outputs[0][after..].iter().all(|v| *v == 0.));
    // The same source slots can be reused after cancellation and natural completion.
    upload(&mut h, 2, 33);
    let operation = start(&mut h, 2, 33);
    h.run(128, 16);
    assert_eq!(h.results[&operation], Outcome::Done);
    h.done(Request::ContentRelease { id: 2 });
    assert!(h.host.endpoint().is_idle());
}

#[shoop_test]
fn removed_bus_or_disconnected_client_cannot_leave_preview_sounding() {
    for disconnect in [false, true] {
        let mut h = Harness::new(&description());
        upload(&mut h, 1, 512);
        h.done(Request::CreateStrip {
            strip: strip(0, 1),
            shape: shape(1, 1, false),
        });
        start(&mut h, 1, 512);
        h.run(64, 16);
        if disconnect {
            h.host.disconnect();
        } else {
            h.done(Request::RemoveBus {
                bus: BusId(SlotId::new(0, 1)),
            });
        }
        h.run(128, 16);
        assert_eq!(*h.outputs[0].last().unwrap(), 0.);
        assert_eq!(
            h.host.process(16).unwrap().phase,
            shoop_rt_engine::Phase::Running
        );
    }
}
