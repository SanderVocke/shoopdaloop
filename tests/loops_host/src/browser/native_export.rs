use super::{allocation_guard, native_graph::transport::Host, BrowserClient};
use crate::{export_wire::*, wire, BrowserExportClient};

#[derive(Debug)]
pub struct ExportReport {
    pub samples: Vec<[f32; 2]>,
    pub requests: Vec<[u8; REQUEST_BYTES]>,
    pub responses: Vec<[u8; RESPONSE_BYTES]>,
    pub primitive: Vec<[u8; 96]>,
    pub journal: Vec<u8>,
    pub semantic: Vec<u8>,
    pub recovered: [u32; 4],
    pub counts: [u32; 4],
    pub storage: [u32; 6],
    pub frames: u64,
    pub callbacks: u64,
}
impl ExportReport {
    pub fn json(&self) -> String {
        format!("{{\"samples\":{:?},\"exportRequests\":{:?},\"exportResponses\":{:?},\"responses\":{:?},\"midiBytes\":{:?},\"semanticBytes\":{:?},\"recovered\":{:?},\"counts\":{:?},\"storage\":{:?},\"frames\":{},\"callbacks\":{}}}", self.samples, self.requests, self.responses, self.primitive, self.journal, self.semantic, self.recovered, self.counts, self.storage, self.frames, self.callbacks)
    }
}
fn collect(
    client: &mut BrowserExportClient,
    host: &mut Host,
    responses: &mut Vec<[u8; RESPONSE_BYTES]>,
) {
    for _ in 0..2 {
        let Some(response) = host.export_response() else {
            break;
        };
        client.response = response;
        let outstanding = client.outstanding();
        client.response[16] ^= 128;
        assert_eq!(client.receive(), 9);
        assert_eq!(client.outstanding(), outstanding);
        client.response = response;
        assert_eq!(client.receive(), 0);
        assert_eq!(client.receive(), 11);
        responses.push(response);
    }
    assert!(host.export_response().is_none());
    assert_eq!(client.outstanding(), 0);
}
fn primitive(client: &mut BrowserClient, host: &mut Host, at: u64) {
    for (kind, frame) in [(1, at), (2, at + 4), (3, at + 12)] {
        assert_eq!(client.prepare(kind, frame, 0, 0), 0);
        assert_eq!(host.primitive(client.request), 0);
    }
}
fn receipts(client: &mut BrowserClient, host: &mut Host, records: &mut Vec<[u8; 96]>, start: u64) {
    let mut terminals = Vec::new();
    for _ in 0..10 {
        let Some(response) = host.primitive_response() else {
            break;
        };
        client.response = response;
        assert_eq!(client.receive(), 0);
        let response_value = wire::Response::decode(&response).unwrap();
        assert_eq!(response_value.error, 0);
        if response_value.phase == wire::Phase::Execution {
            assert_eq!(response_value.status, wire::Status::Applied);
            assert_eq!(response_value.requested, response_value.delivered.unwrap());
            assert_eq!(response_value.observed, response_value.requested);
            terminals.push(response_value.requested);
        }
        records.push(response);
    }
    assert_eq!(terminals, [start, start + 4, start + 12]);
    assert_eq!(client.outstanding(), 0);
}
fn prepare(
    client: &mut BrowserExportClient,
    host: &mut Host,
    requests: &mut Vec<[u8; REQUEST_BYTES]>,
    kind: u8,
    bank: u32,
    at: u64,
    channel: u8,
) {
    assert_eq!(
        client.prepare(
            kind,
            bank,
            1,
            if matches!(kind, AUDIO_READ | MIDI_READ) {
                0
            } else {
                at
            },
            0,
            channel,
            0,
            false
        ),
        0
    );
    if requests.is_empty() {
        let mut malformed = client.request;
        malformed[95] = 1;
        assert_ne!(host.export(malformed), 0);
        let mut foreign = client.request;
        foreign[8] ^= 1;
        assert_eq!(host.export(foreign), 9);
    }
    assert_eq!(host.export(client.request), 0);
    requests.push(client.request);
}
fn step(host: &mut Host, frames: u32) -> super::native_graph::transport::Rendered {
    let rendered = host.render(frames);
    assert_eq!(rendered.result, 0);
    assert_eq!(rendered.heap, allocation_guard::Counts::default());
    rendered
}

pub fn run_exports(threaded: bool, partitions: &[usize]) -> ExportReport {
    assert_eq!(partitions.iter().sum::<usize>(), 128);
    assert!(partitions.iter().all(|n| *n <= 128));
    let mut host = Host::new_exports(u64::MAX, threaded);
    let storage = host.storage();
    assert_eq!(storage, [16384, 64, 96, 1024, 2, 1]);
    let mut client = BrowserExportClient::new(u64::MAX).unwrap();
    let mut control = BrowserClient::new(u64::MAX).unwrap();
    let mut requests = Vec::new();
    let mut responses = Vec::new();
    let mut records = Vec::new();
    let mut samples = Vec::new();
    for (epoch, bank) in [(0, 0), (128, 1)] {
        primitive(&mut control, &mut host, epoch);
        prepare(
            &mut client,
            &mut host,
            &mut requests,
            EXPORT,
            bank,
            epoch + 8,
            0,
        );
        for &frames in partitions {
            let output = step(&mut host, frames as u32);
            samples.extend((0..frames).map(|i| [output.left[i], output.right[i]]));
        }
        collect(&mut client, &mut host, &mut responses);
        receipts(&mut control, &mut host, &mut records, epoch);
        assert!(matches!(host.stop(), Err(15)));
        let metadata = responses.last().unwrap();
        assert_eq!(get32(metadata, 12), APPLIED);
        assert_eq!(get64(metadata, 104), epoch + 8);
        assert_eq!(
            (
                get64(metadata, 136),
                get64(metadata, 144),
                get32(metadata, 152)
            ),
            (u64::from(bank) + 1, 4, 2)
        );
        assert_eq!(
            (get64(metadata, 176), get64(metadata, 184)),
            (epoch + 4, epoch + 4)
        );
        assert_eq!(
            (
                get64(metadata, 192),
                get64(metadata, 200),
                get64(metadata, 208),
                get64(metadata, 216)
            ),
            (epoch, epoch + 4, epoch, epoch + 4)
        );
        assert_eq!(metadata[157], 0);
        assert_eq!(client.retained(), bank + 1);
    }
    let old = crate::run(&[13]);
    for (at, &sample) in samples.iter().enumerate() {
        let base = if at < 128 {
            old.samples.get(at).copied().unwrap_or([0.; 2])
        } else {
            let values = [10., 20., 30., 40., 5., 10., 15., 20., 5., 10., 15., 20.];
            let value = values.get(at - 128).copied().unwrap_or(0.);
            [value, value * 10.]
        };
        assert_eq!(sample, base);
    }
    let mut frame = 256;
    let mut callbacks = 0;
    for bank in 0..2 {
        for kind in [AUDIO_CURSOR, AUDIO_READ, MIDI_CURSOR, MIDI_READ] {
            prepare(&mut client, &mut host, &mut requests, kind, bank, frame, 0);
            let output = step(&mut host, 128);
            frame = output.frames;
            callbacks = output.callbacks;
            collect(&mut client, &mut host, &mut responses);
            let response = responses.last().unwrap();
            assert_eq!(get32(response, 12), APPLIED);
            if kind == AUDIO_READ {
                let expected: Vec<u8> = (1..=4)
                    .flat_map(|i| {
                        let value = i as f32 * if bank == 0 { 1. } else { 10. };
                        [value, value * 10.].into_iter().flat_map(f32::to_le_bytes)
                    })
                    .collect();
                assert_eq!(get32(response, 100), 32);
                assert_eq!(&response[DATA..DATA + 32], expected);
            } else if kind == MIDI_READ {
                let mut expected = Vec::new();
                if bank == 0 {
                    for (at, packet) in [(0u64, [0x90, 60, 100]), (2, [0x80, 60, 45])] {
                        expected.extend(at.to_le_bytes());
                        expected.extend(3u32.to_le_bytes());
                        expected.extend(packet);
                    }
                }
                assert_eq!(get32(response, 100) as usize, expected.len());
                assert_eq!(&response[DATA..DATA + expected.len()], expected);
            }
        }
        for channel in 0..16 {
            prepare(
                &mut client,
                &mut host,
                &mut requests,
                INITIAL,
                bank,
                frame,
                channel,
            );
            let output = step(&mut host, 128);
            frame = output.frames;
            callbacks = output.callbacks;
            collect(&mut client, &mut host, &mut responses);
            let response = responses.last().unwrap();
            assert_eq!(get32(response, 12), APPLIED);
            let mut expected = [0; INITIAL_BYTES];
            expected[0] = channel;
            expected[2..6].fill(255);
            expected[10..12].copy_from_slice(&8192u16.to_le_bytes());
            for key in 0..128 {
                expected[16 + key * 4 + 2] = 255;
                expected[528 + key] = if [64, 66, 69].contains(&key) { 0 } else { 255 };
            }
            assert_eq!(&response[DATA..DATA + INITIAL_BYTES], expected);
        }
    }
    for bank in 0..2 {
        prepare(
            &mut client,
            &mut host,
            &mut requests,
            RELEASE,
            bank,
            frame,
            0,
        );
        let output = step(&mut host, 128);
        frame = output.frames;
        callbacks = output.callbacks;
        collect(&mut client, &mut host, &mut responses);
        assert_eq!(client.generation(bank), 2);
    }
    assert_eq!(host.counts(), [0, 0, 0]);
    let counts = [
        control.outstanding(),
        client.outstanding(),
        client.retained(),
        host.counts()[2],
    ];
    assert_eq!(counts, [0; 4]);
    assert_eq!(host.storage(), storage);
    let stopped = host.stop().unwrap_or_else(|code| panic!("stop {code}"));
    assert_eq!(stopped.recovered, [256, 16, 16, 0]);
    let mut expected_midi = old
        .midi
        .iter()
        .flat_map(|(at, packet)| {
            at.to_le_bytes()
                .into_iter()
                .chain((packet.len() as u32).to_le_bytes())
                .chain(packet.iter().copied())
        })
        .collect::<Vec<_>>();
    let restoration = old
        .midi
        .iter()
        .filter(|(at, packet)| *at == 4 && packet.as_slice() != [0x90, 60, 100])
        .map(|(_, packet)| packet)
        .collect::<Vec<_>>();
    assert_eq!(restoration.len(), 64);
    for at in [132u64, 136] {
        for packet in &restoration {
            expected_midi.extend(at.to_le_bytes());
            expected_midi.extend((packet.len() as u32).to_le_bytes());
            expected_midi.extend(*packet);
        }
    }
    assert_eq!(stopped.journal, expected_midi);
    let mut semantic = old.semantic.clone();
    put32(&mut semantic, 8, 30);
    for record in old.semantic[40..].chunks_exact(160) {
        let mut record: [u8; 160] = record.try_into().unwrap();
        let frame = get64(&record, 8);
        put64(&mut record, 8, frame + 128);
        if record[0] == 2 {
            put64(&mut record, 24, 4);
            put64(&mut record, 32, if frame == 0 { 1 } else { 2 });
            if frame == 0 {
                put64(&mut record, 48, 1);
            }
            put64(&mut record, 64, 2);
            let pulse = get64(&record, 72);
            put64(&mut record, 72, pulse + 128);
        }
        semantic.extend(record);
    }
    assert_eq!(stopped.semantic, semantic);
    ExportReport {
        samples,
        requests,
        responses,
        primitive: records,
        journal: stopped.journal,
        semantic: stopped.semantic,
        recovered: stopped.recovered,
        counts,
        storage,
        frames: frame,
        callbacks,
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn cancelled_reads_eof_stalls_and_128_export_reuses_return_real_owners() {
        use super::*;
        fn rpc(
            client: &mut BrowserExportClient,
            host: &mut Host,
            kind: u8,
            generation: u32,
            at: u64,
            position: u64,
            cancelled: bool,
        ) -> [u8; RESPONSE_BYTES] {
            assert_eq!(
                client.prepare(kind, 0, generation, at, position, 0, 0, cancelled),
                0
            );
            assert_eq!(host.export(client.request), 0);
            step(host, 128);
            let mut responses = Vec::new();
            collect(client, host, &mut responses);
            *responses.last().unwrap()
        }
        let mut host = Host::new_exports(u64::MAX, true);
        let mut client = BrowserExportClient::new(u64::MAX).unwrap();
        let mut control = BrowserClient::new(u64::MAX).unwrap();
        primitive(&mut control, &mut host, 0);
        assert_eq!(client.prepare(EXPORT, 0, 1, 8, 0, 0, 0, false), 0);
        assert_eq!(host.export(client.request), 0);
        step(&mut host, 128);
        collect(&mut client, &mut host, &mut Vec::new());
        receipts(&mut control, &mut host, &mut Vec::new(), 0);
        rpc(&mut client, &mut host, AUDIO_CURSOR, 1, 128, 0, false);
        assert_eq!(client.prepare(AUDIO_READ, 0, 1, 0, 0, 0, 0, true), 0);
        assert_eq!(host.export(client.request), 0);
        step(&mut host, 128);
        let mut busy = client.request;
        let sequence = get64(&busy, 16);
        put64(&mut busy, 16, sequence + 1);
        assert_eq!(host.export(busy), 10);
        assert_eq!(client.outstanding(), 1);
        step(&mut host, 128);
        let mut responses = Vec::new();
        collect(&mut client, &mut host, &mut responses);
        assert_eq!(get32(responses.last().unwrap(), 12), CANCELLED);
        let read = rpc(&mut client, &mut host, AUDIO_READ, 1, 0, 0, false);
        assert_eq!(get32(&read, 100), 32);
        let eof = rpc(&mut client, &mut host, AUDIO_READ, 1, 0, 32, false);
        assert_eq!((get32(&eof, 100), get32(&eof, 8)), (0, 4));
        let now = host.render(0).frames;
        let cancelled = rpc(&mut client, &mut host, RELEASE, 1, now, 0, true);
        assert_eq!(get32(&cancelled, 12), CANCELLED);
        assert_eq!(client.retained(), 1);
        assert!(matches!(host.stop(), Err(15)));
        let now = host.render(0).frames;
        rpc(&mut client, &mut host, RELEASE, 1, now, 0, false);
        for _ in 0..128 {
            let generation = client.generation(0);
            let now = host.render(0).frames;
            rpc(&mut client, &mut host, EXPORT, generation, now, 0, false);
            rpc(
                &mut client,
                &mut host,
                RELEASE,
                generation,
                now + 128,
                0,
                false,
            );
        }
        assert_eq!(client.generation(0), 130);
        assert_eq!(client.retained(), 0);
        assert_eq!(host.counts(), [0; 3]);
        assert_eq!(
            host.stop()
                .unwrap_or_else(|code| panic!("stop {code}"))
                .recovered,
            [256, 16, 16, 0]
        );
    }

    #[test]
    fn immutable_exports_hold_both_revisions_across_native_direct_and_bounded_threads() {
        let reference = super::run_exports(false, &[128]);
        for partitions in [vec![128], vec![1; 128], vec![0, 3, 0, 5, 120, 0]] {
            let other = super::run_exports(true, &partitions);
            assert_eq!(reference.samples, other.samples);
            assert_eq!(reference.requests, other.requests);
            assert_eq!(reference.responses.len(), other.responses.len());
            for (a, b) in reference.responses.iter().zip(&other.responses) {
                let mut a = *a;
                let mut b = *b;
                if reference.callbacks != other.callbacks
                    && a[6] == super::EXPORT
                    && super::get32(&a, 12) == super::APPLIED
                {
                    // Resource append revisions are not the loop content revision.
                    assert!(super::get64(&a, 224) > 0 && super::get64(&b, 224) > 0);
                    assert!(super::get64(&a, 232) > 0 && super::get64(&b, 232) > 0);
                    a[224..240].fill(0);
                    b[224..240].fill(0);
                }
                assert_eq!(a, b);
            }
            assert_eq!(reference.primitive, other.primitive);
            assert_eq!(reference.journal, other.journal);
            assert_eq!(reference.semantic, other.semantic);
            assert_eq!(reference.recovered, other.recovered);
            assert_eq!(reference.counts, other.counts);
            assert_eq!(reference.frames, other.frames);
        }
    }
}
