use super::*;
fn request(kind: u8) -> Request {
    Request {
        session: u64::MAX,
        sequence: u64::MAX,
        kind,
        at: if matches!(kind, AUDIO_READ | MIDI_READ) {
            0
        } else {
            u64::MAX
        },
        bank: 1,
        generation: u32::MAX,
        target: (kind == EXPORT).then(|| LoopId::from_parts(u32::MAX - 1, u32::MAX).unwrap()),
        position: if matches!(kind, AUDIO_READ | MIDI_READ) {
            u64::MAX
        } else {
            0
        },
        channel: if kind == INITIAL { 15 } else { 0 },
        selection: if kind == EXPORT { 2 } else { 0 },
        cancelled: true,
    }
}
#[test]
fn full_width_export_records_reject_truncation_padding_and_invalid_projection() {
    for kind in EXPORT..=RELEASE {
        let request = request(kind);
        let bytes = request.encode().unwrap();
        assert_eq!(Request::decode(&bytes).unwrap(), request);
        for end in 0..REQUEST_BYTES {
            assert!(Request::decode(&bytes[..end]).is_err());
        }
        for offset in [32, 33, 34, 35, 54, 55, 64, 80, 95] {
            let mut bad = bytes;
            bad[offset] = 1;
            assert!(Request::decode(&bad).is_err());
        }
        let mut oversized = bytes.to_vec();
        oversized.push(0);
        assert!(Request::decode(&oversized).is_err());
    }
    let report = crate::run_exports(false, &[128]);
    for bytes in &report.responses {
        assert_eq!(Response::decode(bytes).unwrap().0, *bytes);
        for end in 0..RESPONSE_BYTES {
            assert!(Response::decode(&bytes[..end]).is_err());
        }
        for offset in [0, 4, 6, 7, 8] {
            let mut bad = *bytes;
            bad[offset] = 255;
            assert!(Response::decode(&bad).is_err());
        }
        let mut bad = bytes.to_vec();
        bad.push(0);
        assert!(Response::decode(&bad).is_err());
    }
    let initial = report
        .responses
        .iter()
        .find(|b| b[6] == INITIAL && get32(*b, 12) == APPLIED)
        .unwrap();
    for offset in [
        DATA + 1,
        DATA + 6,
        DATA + 7,
        DATA + 8,
        DATA + 9,
        DATA + 11,
        DATA + 15,
        DATA + 16,
        DATA + 17,
        DATA + 18,
        DATA + 19,
        DATA + 528,
        DATA + INITIAL_BYTES,
    ] {
        let mut bad = *initial;
        bad[offset] = if offset == DATA + 18 || offset == DATA + 528 {
            128
        } else {
            255
        };
        assert!(
            Response::decode(&bad).is_err(),
            "projection offset {offset}"
        );
    }
    let exported = report
        .responses
        .iter()
        .find(|b| b[6] == EXPORT && get32(*b, 12) == APPLIED)
        .unwrap();
    for offset in [156, 157, 158, 159, 152] {
        let mut bad = *exported;
        bad[offset] = 255;
        assert!(Response::decode(&bad).is_err());
    }
}
#[test]
fn client_identity_credit_and_exhaustion_are_transactional_and_never_wrap() {
    let mut client = Client::new(u64::MAX).unwrap();
    client.last = u64::MAX - 1;
    let mut input = request(EXPORT);
    input.bank = 0;
    input.generation = 1;
    let bytes = client.prepare(input).unwrap();
    let input = Request::decode(&bytes).unwrap();
    assert_eq!(input.sequence, u64::MAX);
    let cancelled = Response::new(input, 1, CANCELLED, 1, 0);
    for offset in [16, 24, 76, 80] {
        let mut bad = cancelled;
        bad.0[offset] ^= 1;
        assert!(client.receive(&bad.0).is_err());
        assert_eq!(client.outstanding(), 1);
        assert_eq!(client.retained(), 0);
    }
    client.receive(&cancelled.0).unwrap();
    assert_eq!(client.prepare(input), Err(Error::Exhausted));
    let mut client = Client::new(u64::MAX).unwrap();
    client.banks[0] = Bank {
        generation: u32::MAX,
        active: true,
        ..Bank::default()
    };
    let mut release = request(RELEASE);
    release.bank = 0;
    release.at = 0;
    let bytes = client.prepare(release).unwrap();
    let release = Request::decode(&bytes).unwrap();
    client
        .receive(&Response::new(release, 1, ADMITTED, 7, 0).0)
        .unwrap();
    let mut returned = Response::new(release, 2, APPLIED, 7, 0);
    put32(&mut returned.0, 8, 1);
    client.receive(&returned.0).unwrap();
    assert_eq!(client.generation(0), Some(0));
    assert_eq!(client.retained(), 0);
    let mut input = request(EXPORT);
    input.bank = 0;
    input.generation = 0;
    assert_eq!(client.prepare(input), Err(Error::Identity));
}
