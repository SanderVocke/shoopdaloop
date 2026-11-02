mod support;
use shoop_rt_protocol::*;
use shoop_wasm_test_support::shoop_test;
use support::*;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn begin(id: u64, midi: bool, frames: u64, bytes: u64) -> Request {
    Request::ContentBegin(ContentDescription {
        id,
        midi,
        frames,
        bytes,
    })
}
fn write(id: u64, offset: u64, bytes: &[u8]) -> Request {
    Request::ContentWrite {
        id,
        offset,
        data: ContentChunk::new(bytes).unwrap(),
    }
}

#[shoop_test]
fn uploads_are_inaudible_bounded_and_recycle_resources_while_existing_audio_continues() {
    let mut h = Harness::new(&description());
    h.input = Box::new(|_, _| 1.);
    let strip = strip(0, 1);
    h.done(Request::CreateStrip {
        strip,
        shape: shape(1, 1, false),
    });
    h.done(Request::Connect {
        port: port(strip, PortKind::AudioIn, 0),
        host: audio_in(0),
        connected: true,
    });
    h.done(route(strip, 0, 0, true));
    h.done(Request::StripControl {
        strip,
        control: StripControl::Monitoring(true),
    });
    h.run(8, 4);
    let start = h.outputs[0].len();
    for id in 1..=32 {
        h.done(begin(id, false, 32, 256));
        assert_eq!(
            h.apply(write(id, 1, &[0; 8])),
            Outcome::Refused(Refusal::InvalidValue)
        );
        assert_eq!(
            h.apply(Request::ContentFinish { id }),
            Outcome::Refused(Refusal::InvalidValue)
        );
        h.done(write(id, 0, &[0; 248]));
        h.done(write(id, 248, &[0; 8]));
        h.done(Request::ContentFinish { id });
        h.done(Request::ContentFinish { id });
        h.done(Request::ContentRelease { id });
        assert_eq!(
            h.apply(begin(id, false, 0, 0)),
            Outcome::Refused(Refusal::InvalidId)
        );
    }
    assert!(h.outputs[0][start..].iter().all(|&value| value == 1.));
    assert_eq!(
        h.apply(begin(33, false, u64::MAX, 0)),
        Outcome::Refused(Refusal::InvalidValue)
    );
    assert_eq!(
        h.apply(begin(33, false, 1, 4)),
        Outcome::Refused(Refusal::InvalidValue)
    );
    assert_eq!(
        h.apply(begin(33, false, 0, content::MAX_CONTENT_BYTES + 1)),
        Outcome::Refused(Refusal::InvalidValue)
    );
    h.done(begin(33, false, 0, 0));
    h.done(Request::ContentFinish { id: 33 });
    h.done(Request::ContentRelease { id: 33 });
}

#[shoop_test]
fn pipelined_chunks_and_disconnect_cleanup_return_every_prepared_owner() {
    pipeline_cycles(8, 8);
}

#[shoop_test]
fn wide_upload_window_and_disconnect_reuse_every_prepared_owner() {
    pipeline_cycles(shoop_rt_protocol::content::CONTENT_WRITE_WINDOW, 128);
}

#[shoop_test]
fn small_engines_prepare_a_full_wire_chunk_without_exceeding_page_or_validation_limits() {
    let mut settings = description();
    settings.page_bytes = 4096;
    let mut h = Harness::new(&settings);
    let Some(EngineMessage::Welcome { limits, .. }) = h.lifecycle.first() else {
        panic!("expected engine limits");
    };
    assert_eq!(
        usize::from(limits.content_chunk_bytes),
        content::CONTENT_CHUNK_BYTES
    );
    let bytes = vec![0; content::CONTENT_CHUNK_BYTES];
    h.done(begin(1, false, bytes.len() as u64 / 8, bytes.len() as u64));
    h.done(write(1, 0, &bytes));
    h.done(Request::ContentFinish { id: 1 });
    h.done(Request::ContentRelease { id: 1 });
    let mut midi = Vec::new();
    for frame in 0..64u64 {
        midi.extend_from_slice(&frame.to_le_bytes());
        midi.extend_from_slice(&3u32.to_le_bytes());
        midi.extend_from_slice(&[0x90, 60, 64]);
    }
    h.done(begin(2, true, 64, midi.len() as u64));
    h.done(write(2, 0, &midi));
    h.done(Request::ContentFinish { id: 2 });
    h.done(Request::ContentRelease { id: 2 });
}

fn pipeline_cycles(window: usize, credits: u16) {
    let mut settings = description();
    settings.reliable_credits = credits;
    let mut h = Harness::new(&settings);
    for cycle in 0..24 {
        let id = cycle + 1;
        h.done(begin(id, false, window as u64 * 32, window as u64 * 256));
        let requests = (0..window)
            .map(|chunk| h.send(write(id, chunk as u64 * 256, &[0; 256])))
            .collect::<Vec<_>>();
        if cycle % 2 == 0 {
            h.settle();
            h.done(Request::ContentFinish { id });
            for request in requests {
                assert_eq!(h.results[&request], Outcome::Done);
            }
            h.done(Request::ContentRelease { id });
        } else {
            assert_no_heap(|| h.host.disconnect());
            h.pump();
            assert_no_heap(|| h.host.connect()).unwrap();
            h.pump();
            h.settle();
            assert_eq!(
                h.apply(write(id, 0, &[0; 8])),
                Outcome::Refused(Refusal::InvalidId)
            );
        }
    }
    h.done(begin(25, false, 256, 2048));
    h.done(Request::ContentRelease { id: 25 });
}

#[shoop_test]
fn midi_validation_failures_and_abandoned_uploads_do_not_exhaust_prepared_slots() {
    let mut h = Harness::new(&description());
    for id in 1..=32 {
        h.done(begin(id, true, 4, 15));
        let mut packet = Vec::new();
        packet.extend_from_slice(&0u64.to_le_bytes());
        packet.extend_from_slice(&3u32.to_le_bytes());
        packet.extend_from_slice(if id % 2 == 0 {
            &[0x90, 60, 100]
        } else {
            &[0x90, 60, 255]
        });
        let written = h.apply(write(id, 0, &packet));
        if id % 2 == 0 {
            assert_eq!(written, Outcome::Done);
            h.done(Request::ContentFinish { id });
            h.done(Request::ContentRelease { id });
        } else {
            assert!(matches!(written, Outcome::Refused(_)));
            h.settle();
        }
    }
    for id in 33..=64 {
        h.done(begin(id, false, 16, 128));
        h.done(write(id, 0, &[0; 64]));
        h.done(Request::ContentRelease { id });
    }
    h.done(begin(65, true, 0, 0));
    h.done(Request::ContentFinish { id: 65 });
    h.done(Request::ContentRelease { id: 65 });
}
