use super::{native_graph::transport::Host, BrowserClient};
use crate::{
    allocation_guard,
    export_wire::{get32, get64},
    staging_wire::*,
    BrowserStagingClient,
};
mod expected;
#[derive(Debug, PartialEq)]
pub struct StagingReport {
    pub samples: Vec<[f32; 2]>,
    pub requests: Vec<[u8; REQUEST_BYTES]>,
    pub replies: Vec<[u8; RESPONSE_BYTES]>,
    pub primitive: Vec<[u8; 96]>,
    pub midi: Vec<u8>,
    pub semantic: Vec<u8>,
    pub recovered: [u32; 4],
    pub counts: [u32; 4],
    pub records: u32,
    pub frames: u64,
    pub callbacks: u64,
    pub program_frame: u64,
}
impl StagingReport {
    pub fn json(&self) -> String {
        format!("{{\"samples\":{:?},\"stagingRequests\":{:?},\"stagingResponses\":{:?},\"responses\":{:?},\"midiBytes\":{:?},\"semanticBytes\":{:?},\"recovered\":{:?},\"counts\":{:?},\"recordCount\":{},\"frames\":{},\"callbacks\":{},\"programFrame\":{}}}",self.samples,self.requests,self.replies,self.primitive,self.midi,self.semantic,self.recovered,self.counts,self.records,self.frames,self.callbacks,self.program_frame)
    }
}
struct Driver {
    host: Host,
    client: BrowserStagingClient,
    requests: Vec<[u8; REQUEST_BYTES]>,
    replies: Vec<[u8; RESPONSE_BYTES]>,
    frame: u64,
    callbacks: u64,
    cancelled: bool,
    last_audio: [[f32; 128]; 2],
}
impl Driver {
    fn rpc(&mut self, kind: u8, bank: u32, args: [u64; 3], index: u32) -> [u8; RESPONSE_BYTES] {
        let response = self.request(kind, bank, args, index);
        assert_eq!(get32(&response, 64), APPLIED, "kind {kind}: {response:?}");
        response
    }
    fn request(&mut self, kind: u8, bank: u32, args: [u64; 3], index: u32) -> [u8; RESPONSE_BYTES] {
        self.timed(
            kind,
            bank,
            args,
            index,
            if resource(kind) { 0 } else { self.frame },
            128,
        )
    }
    fn timed(
        &mut self,
        kind: u8,
        bank: u32,
        args: [u64; 3],
        index: u32,
        at: u64,
        frames: u32,
    ) -> [u8; RESPONSE_BYTES] {
        let generation = self.client.generation(bank, program(kind));
        assert_eq!(
            self.client.prepare(
                kind,
                bank,
                generation,
                at,
                args[0],
                args[1],
                args[2],
                index,
                std::mem::take(&mut self.cancelled)
            ),
            0
        );
        let packet = self.client.request;
        if self.requests.is_empty() {
            let mut bad = packet;
            bad[0] ^= 1;
            assert_eq!(self.host.staging(bad), 2);
            let mut foreign = packet;
            foreign[8] ^= 128;
            assert_eq!(self.host.staging(foreign), 9);
        }
        assert_eq!(self.host.staging(packet), 0);
        self.requests.push(packet);
        let rendered = self.host.render(frames);
        assert_eq!(rendered.result, 0, "render kind {kind} at {}", self.frame);
        assert_eq!(rendered.heap, allocation_guard::Counts::default());
        assert_eq!(rendered.frames, self.frame + u64::from(frames));
        self.frame = rendered.frames;
        self.callbacks = rendered.callbacks;
        self.last_audio = [rendered.left, rendered.right];
        let start = self.replies.len();
        for _ in 0..2 {
            let Some(packet) = self.host.staging_response() else {
                break;
            };
            self.client.response = packet;
            self.client.response[8] ^= 128;
            assert_eq!(self.client.receive(), 9);
            self.client.response = packet;
            assert_eq!(self.client.receive(), 0);
            assert_eq!(self.client.receive(), 8);
            self.replies.push(packet);
        }
        assert!(self.replies.len() > start);
        assert!(self.host.staging_response().is_none());
        assert_eq!(self.client.outstanding(), 0);
        *self.replies.last().unwrap()
    }
    fn audio_data(&mut self, value: f32) {
        self.rpc(CURSOR, 0, [0; 3], 0);
        let data = self.rpc(READ, 0, [0; 3], 0);
        assert_eq!((get32(&data, 128), get32(&data, 84)), (64, 2));
        for frame in 0..8 {
            for channel in 0..2 {
                let at = DATA + (frame * 2 + channel) * 4;
                assert_eq!(
                    f32::from_le_bytes(data[at..at + 4].try_into().unwrap()),
                    (value + frame as f32) * if channel == 0 { 1. } else { 10. }
                );
            }
        }
    }
}
pub fn run(threaded: bool) -> StagingReport {
    let mut driver = Driver {
        host: Host::new_staging(u64::MAX, threaded),
        client: BrowserStagingClient::new(u64::MAX).unwrap(),
        requests: vec![],
        replies: vec![],
        frame: 0,
        callbacks: 0,
        cancelled: false,
        last_audio: [[0.; 128]; 2],
    };
    for index in [driver.client.record_count(), 1 << 31, u32::MAX] {
        assert_eq!(
            driver
                .client
                .prepare(APPEND, 0, 1, 0, 0, 0, 0, index, false),
            7
        );
        assert_eq!(driver.client.outstanding(), 0);
    }
    let mut primitive = BrowserClient::new(u64::MAX).unwrap();
    for (kind, at) in [(1, 0), (2, 4), (3, 12)] {
        assert_eq!(primitive.prepare(kind, at, 0, 0), 0);
        assert_eq!(driver.host.primitive(primitive.request), 0);
    }
    driver.rpc(CREATE, 0, [0; 3], 0);
    let mut samples = (0..128)
        .map(|i| [driver.last_audio[0][i], driver.last_audio[1][i]])
        .collect::<Vec<_>>();
    let mut receipts = vec![];
    for _ in 0..6 {
        let packet = driver.host.primitive_response().expect("primitive receipt");
        primitive.response = packet;
        assert_eq!(primitive.receive(), 0);
        receipts.push(packet);
    }
    assert!(driver.host.primitive_response().is_none());
    assert!(matches!(driver.host.stop(), Err(15)));
    for (kind, args, index) in [
        (BEGIN, [0, 8, 64], 0),
        (WRITE, [0; 3], 0),
        (FINISH, [0; 3], 0),
        (COMMIT, [0; 3], 0),
        (SNAPSHOT, [0; 3], 0),
    ] {
        driver.rpc(kind, 0, args, index);
    }
    driver.audio_data(10.);
    for (kind, args, index) in [
        (BEGIN, [1, 8, 64], 0),
        (WRITE, [0; 3], 2),
        (FINISH, [0; 3], 0),
        (COMMIT, [0; 3], 0),
    ] {
        driver.rpc(kind, 0, args, index);
    }
    driver.audio_data(10.);
    driver.rpc(RELEASE, 0, [0; 3], 0);
    let snapshot = driver.rpc(SNAPSHOT, 0, [0; 3], 0);
    assert_eq!(get64(&snapshot, 88), 2);
    driver.audio_data(100.);
    driver.rpc(SOURCE, 0, [0; 3], 0);
    let ready = driver.rpc(VALIDATE_SOURCE, 0, [0; 3], 0);
    assert_eq!(get64(&ready, 88), 1);
    driver.rpc(RELEASE_SOURCE, 0, [0; 3], 0);
    driver.rpc(RELEASE, 0, [0; 3], 0);
    driver.rpc(REMOVE, 0, [0; 3], 0);
    for (kind, args, index) in [
        (CREATE, [0; 3], 0),
        (BEGIN, [0, 8, 64], 1),
        (WRITE, [0; 3], 1),
        (FINISH, [0; 3], 0),
        (COMMIT, [0; 3], 0),
        (SNAPSHOT, [0; 3], 0),
        (CURSOR, [0; 3], 0),
    ] {
        driver.rpc(kind, 1, args, index);
    }
    let midi = driver.rpc(READ, 1, [0; 3], 0);
    assert_eq!(&midi[DATA + 12..DATA + 15], &[0x90, 61, 90]);
    assert_eq!(get64(&midi, DATA + 15), 4);
    assert_eq!(get32(&midi, DATA + 23), 37);
    assert_eq!(midi[DATA + 27], 0xf0);
    assert_eq!(midi[DATA + 63], 0xf7);
    for i in 0..35 {
        assert_eq!(midi[DATA + 28 + i], i as u8);
    }
    let refused = driver.request(SOURCE, 1, [0; 3], 0);
    assert_eq!(
        (
            get32(&refused, 64),
            get32(&refused, 68),
            get32(&refused, 132)
        ),
        (FAILED, 522, 5)
    );
    driver.rpc(RELEASE, 1, [0; 3], 0);
    driver.rpc(REMOVE, 1, [0; 3], 0);
    driver.rpc(PROGRAM_BEGIN, 0, [0; 3], 0);
    let records = driver.client.record_count();
    for record in 0..records {
        driver.rpc(APPEND, 0, [u64::from(record), 0, 0], record);
    }
    let mut steps = 0;
    loop {
        let progress = driver.rpc(VALIDATE, 0, [0; 3], 0);
        if get64(&progress, 88) == 1 {
            break;
        }
        let total = get64(&progress, 96);
        assert!(total > steps);
        steps = total;
    }
    let activated = driver.rpc(ACTIVATE, 0, [1, 0, 0], 0);
    assert_eq!((get64(&activated, 88), get64(&activated, 96)), (1, 1));
    driver.rpc(RECYCLE, 0, [0; 3], 0);
    let program_frame = driver.frame;
    for (kind, iteration, at, frames) in [
        (PLAY, 0, program_frame, 1),
        (SEEK_ITERATION, 1, program_frame + 4, 4),
        (STOP, 0, program_frame + 8, 4),
    ] {
        let reply = driver.timed(kind, 0, [1 | (1u64 << 32), iteration, 0], 0, at, frames);
        assert_eq!((get32(&reply, 64), get64(&reply, 88)), (APPLIED, at));
        samples.extend(
            (0..frames as usize).map(|i| [driver.last_audio[0][i], driver.last_audio[1][i]]),
        );
    }
    let baseline = crate::run(&[13]);
    let expected = baseline
        .samples
        .iter()
        .copied()
        .chain(std::iter::repeat([0.; 2]).take(115))
        .chain((0..9).map(|i| {
            if i < 8 {
                let value = (i % 4 + 1) as f32 * 0.5;
                [value, value * 10.]
            } else {
                [0.; 2]
            }
        }))
        .collect::<Vec<_>>();
    assert_eq!(samples, expected);
    let receiver_counts = driver.host.staging_counts();
    assert_eq!(receiver_counts, [0; 2]);
    assert_eq!(driver.client.retained(), 0);
    assert_eq!(driver.client.generation(0, true), 2);
    assert_eq!(driver.client.generation(0, false), 2);
    assert_eq!(driver.client.generation(1, false), 2);
    let stopped = driver
        .host
        .stop()
        .unwrap_or_else(|code| panic!("stop {code}"));
    assert_eq!(stopped.recovered, [256, 16, 16, 0]);
    let mut expected_midi = vec![];
    for (at, packet) in &baseline.midi {
        expected_midi.extend(at.to_le_bytes());
        expected_midi.extend((packet.len() as u32).to_le_bytes());
        expected_midi.extend(packet);
    }
    for (at, packet) in baseline.midi.iter().filter(|(at, _)| *at >= 4) {
        expected_midi.extend((program_frame + at - 4).to_le_bytes());
        expected_midi.extend((packet.len() as u32).to_le_bytes());
        expected_midi.extend(packet);
    }
    assert_eq!(stopped.journal, expected_midi);
    assert_eq!(stopped.semantic, expected::semantic(program_frame));
    StagingReport {
        samples,
        requests: driver.requests,
        replies: driver.replies,
        primitive: receipts,
        midi: stopped.journal,
        semantic: stopped.semantic,
        recovered: stopped.recovered,
        counts: [
            primitive.outstanding(),
            driver.client.outstanding(),
            driver.client.retained(),
            receiver_counts[1],
        ],
        records,
        frames: driver.frame,
        callbacks: driver.callbacks,
        program_frame,
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn prepared_resource_uploads_immutable_reads_and_nested_programs_match_bounded_actors() {
        assert_eq!(super::run(false), super::run(true));
    }
    #[test]
    fn cancelled_owning_chunks_and_128_program_reuses_preserve_real_buffers_and_generations() {
        use super::*;
        for threaded in [false, true] {
            let mut d = Driver {
                host: Host::new_staging(u64::MAX, threaded),
                client: BrowserStagingClient::new(u64::MAX).unwrap(),
                requests: vec![],
                replies: vec![],
                frame: 0,
                callbacks: 0,
                cancelled: false,
                last_audio: [[0.; 128]; 2],
            };
            d.rpc(CREATE, 0, [0; 3], 0);
            d.rpc(BEGIN, 0, [0, 8, 64], 0);
            d.cancelled = true;
            let cancelled = d.request(WRITE, 0, [0; 3], 0);
            assert_eq!(
                (
                    get32(&cancelled, 64),
                    get32(&cancelled, 128),
                    get32(&cancelled, 132)
                ),
                (CANCELLED, 64, 3)
            );
            d.rpc(WRITE, 0, [0; 3], 0);
            d.rpc(FINISH, 0, [0; 3], 0);
            d.rpc(COMMIT, 0, [0; 3], 0);
            d.rpc(SNAPSHOT, 0, [0; 3], 0);
            d.rpc(CURSOR, 0, [0; 3], 0);
            d.cancelled = true;
            let cancelled = d.request(READ, 0, [0; 3], 0);
            assert_eq!(
                (get32(&cancelled, 64), get32(&cancelled, 128)),
                (CANCELLED, 0)
            );
            d.audio_data(10.);
            assert!(matches!(d.host.stop(), Err(15)));
            d.rpc(RELEASE, 0, [0; 3], 0);
            d.rpc(REMOVE, 0, [0; 3], 0);
            d.cancelled = true;
            let cancelled = d.request(PROGRAM_BEGIN, 0, [0; 3], 0);
            assert_eq!(
                (get32(&cancelled, 64), get32(&cancelled, 128)),
                (CANCELLED, 64)
            );
            d.rpc(PROGRAM_BEGIN, 0, [0; 3], 0);
            d.cancelled = true;
            let cancelled = d.request(APPEND, 0, [0; 3], 0);
            assert_eq!(
                (get32(&cancelled, 64), get32(&cancelled, 128)),
                (CANCELLED, 64)
            );
            for record in 0..d.client.record_count() {
                d.rpc(APPEND, 0, [u64::from(record), 0, 0], record);
            }
            let ready = d.rpc(VALIDATE, 0, [0; 3], 0);
            assert_eq!(get64(&ready, 88), 1);
            d.rpc(CANCEL, 0, [0; 3], 0);
            for _ in 0..128 {
                d.rpc(PROGRAM_BEGIN, 0, [0; 3], 0);
                d.rpc(CANCEL, 0, [0; 3], 0);
            }
            assert_eq!(d.client.generation(0, true), 130);
            assert_eq!(d.client.generation(0, false), 2);
            assert_eq!(d.client.retained(), 0);
            assert_eq!(d.host.staging_counts(), [0; 2]);
            assert_eq!(d.host.stop().unwrap().recovered, [256, 16, 16, 0]);
        }
    }
}
