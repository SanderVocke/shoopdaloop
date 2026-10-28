use super::*;
fn request(kind: u8) -> Request {
    Request {
        session: u64::MAX,
        sequence: u64::MAX,
        kind,
        cancelled: false,
        at: 0,
        bank: 1,
        generation: u32::MAX,
        args: [0; 3],
        data: [0; 64],
    }
}
#[test]
fn canonical_staging_records_preserve_full_width_and_reject_all_truncations_and_padding() {
    for kind in [
        CREATE,
        BEGIN,
        WRITE,
        FINISH,
        COMMIT,
        SNAPSHOT,
        CURSOR,
        READ,
        RELEASE,
        REMOVE,
        CANCEL_UPLOAD,
        SOURCE,
        VALIDATE_SOURCE,
        RELEASE_SOURCE,
        PROGRAM_BEGIN,
        APPEND,
        VALIDATE,
        ACTIVATE,
        RECYCLE,
        CANCEL,
        PLAY,
        STOP,
        STATUS,
        SEEK_ITERATION,
        SELECT,
        RECORD_AFTER,
        QUERY_STATE,
    ] {
        let mut r = request(kind);
        if kind == BEGIN {
            r.data[0] = 1;
            put32(&mut r.data, 4, 2);
            r.args = [u64::MAX; 3];
        }
        if matches!(kind, WRITE | READ | APPEND | PLAY | STOP | QUERY_STATE) {
            r.args[0] = u64::MAX;
        }
        if matches!(kind, ACTIVATE | SEEK_ITERATION | SELECT) {
            r.args = [u64::MAX, u64::MAX, 0];
        }
        if kind == RECORD_AFTER {
            r.args = [u64::MAX; 3];
        }
        if !resource(kind) {
            r.at = u64::MAX;
        }
        if matches!(kind, WRITE | PROGRAM_BEGIN | APPEND) {
            r.data = [0xa5; 64];
        }
        let bytes = r.encode().unwrap();
        assert_eq!(Request::decode(&bytes), Ok(r));
        for length in 0..REQUEST_BYTES {
            assert!(Request::decode(&bytes[..length]).is_err());
        }
        for at in 0..REQUEST_BYTES {
            if bytes[at] == 0 {
                let mut bad = bytes;
                bad[at] = 1;
                if at >= 64 && !matches!(kind, BEGIN | WRITE | PROGRAM_BEGIN | APPEND)
                    || at >= 40
                        && at < 64
                        && at
                            >= 40
                                + 8 * match kind {
                                    BEGIN => 3,
                                    WRITE | READ | APPEND | PLAY | STOP | QUERY_STATE => 1,
                                    ACTIVATE | SEEK_ITERATION | SELECT => 2,
                                    RECORD_AFTER => 3,
                                    _ => 0,
                                }
                {
                    assert!(Request::decode(&bad).is_err(), "kind {kind} byte {at}");
                }
            }
        }
    }
    let mut midi = request(BEGIN);
    midi.data[0] = 2;
    put32(&mut midi.data, 4, 1);
    assert_eq!(midi.encode(), Err(Error::Reserved));
    let report = crate::run_staging(false);
    for packet in report.replies {
        assert!(Response::decode(&packet).is_ok());
        for length in 0..RESPONSE_BYTES {
            assert!(Response::decode(&packet[..length]).is_err());
        }
        let len = get32(&packet, 128) as usize;
        for at in (136..DATA).chain(DATA + len..RESPONSE_BYTES) {
            let mut bad = packet;
            bad[at] = 1;
            assert!(Response::decode(&bad).is_err(), "result padding {at}");
        }
    }
}
#[test]
fn countdown_results_keep_delivery_distinct_from_terminal_frame_and_preserve_credit_on_corruption()
{
    let mut client = Client::new(u64::MAX).unwrap();
    let mut r = request(RECORD_AFTER);
    r.bank = 0;
    r.generation = 1;
    r.at = 7;
    r.args = [1u64 << 32, (1u64 << 32) | 1, 2];
    let r = Request::decode(&client.prepare(r).unwrap()).unwrap();
    client
        .receive(&Response::new(r, 1, ADMITTED, 17, 7, 7).0)
        .unwrap();
    let mut applied = Response::new(r, 2, APPLIED, 17, 7, 9);
    put32(&mut applied.0, 84, 1);
    put64(&mut applied.0, 48, 7);
    put64(&mut applied.0, 88, 9);
    for at in [24, 32, 40, 48, 56, 88] {
        let mut bad = applied.0;
        bad[at] ^= 1;
        assert!(client.receive(&bad).is_err());
        assert_eq!(client.outstanding(), 1);
    }
    for kind in [PLAY, SELECT, QUERY_STATE] {
        let mut bad = applied.0;
        bad[6] = kind;
        assert!(Response::decode(&bad).is_err());
    }
    client.receive(&applied.0).unwrap();
    assert_eq!(client.outstanding(), 0);
    let last = client.last;
    for index in [2, 1u64 << 32, u64::MAX] {
        let mut select = r;
        select.kind = SELECT;
        select.args = [1u64 << 32, index, 0];
        assert_eq!(client.prepare(select), Err(Error::Slot));
        assert_eq!(client.last, last);
        assert_eq!(client.outstanding(), 0);
    }
}
#[test]
fn staging_client_rejections_preserve_credit_and_nonwrapping_identity_watermarks() {
    let mut client = Client::new(u64::MAX).unwrap();
    client.last = u64::MAX - 1;
    client.banks[1][1].generation = u32::MAX;
    let r = Request::decode(&client.prepare(request(PROGRAM_BEGIN)).unwrap()).unwrap();
    assert_eq!(r.sequence, u64::MAX);
    let mut admitted = Response::new(r, 1, ADMITTED, u64::MAX, 0, 0);
    put32(&mut admitted.0, 128, 64);
    admitted.0[DATA..DATA + 64].copy_from_slice(&r.data);
    for at in [6, 8, 16, 32, 72, 76, 80, 88, 132, DATA] {
        let mut bad = admitted.0;
        bad[at] ^= 1;
        assert!(client.receive(&bad).is_err(), "identity {at}");
        assert_eq!(client.outstanding(), 1);
        assert!(client.pending.as_ref().unwrap().runtime.is_none());
    }
    client.receive(&admitted.0).unwrap();
    assert_eq!(client.receive(&admitted.0), Err(Error::Sequence));
    let mut applied = Response::new(r, 2, APPLIED, u64::MAX, 0, 0);
    put32(&mut applied.0, 84, 1);
    put32(&mut applied.0, 132, 256);
    client.receive(&applied.0).unwrap();
    assert_eq!(client.outstanding(), 0);
    assert_eq!(client.retained(), 1);
    assert_eq!(client.prepare(request(CANCEL)), Err(Error::Exhausted));
    assert_eq!(client.retained(), 1);
    client.last = 0;
    let r = Request::decode(&client.prepare(request(CANCEL)).unwrap()).unwrap();
    let admitted = Response::new(r, 1, ADMITTED, 1, 0, 0);
    let mut admitted = admitted;
    put32(&mut admitted.0, 132, 256);
    client.receive(&admitted.0).unwrap();
    let mut applied = Response::new(r, 2, APPLIED, 1, 0, 0);
    put32(&mut applied.0, 84, 1);
    put32(&mut applied.0, 80, 0);
    client.receive(&applied.0).unwrap();
    assert_eq!(client.generation(1, true), Some(0));
    assert_eq!(client.retained(), 0);
    assert_eq!(client.prepare(request(PROGRAM_BEGIN)), Err(Error::Identity));
}
