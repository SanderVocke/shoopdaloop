use super::{native_graph::transport::Host, BrowserClient};
use crate::{
    allocation_guard, export_wire as e, staging_wire as s, BrowserExportClient,
    BrowserStagingClient,
};
mod expected;

#[derive(Debug, PartialEq)]
pub struct MediaReport {
    pub samples: Vec<[f32; 2]>,
    pub requests: Vec<[u8; 64]>,
    pub responses: Vec<[u8; 96]>,
    pub staging_requests: Vec<[u8; s::REQUEST_BYTES]>,
    pub staging_responses: Vec<[u8; s::RESPONSE_BYTES]>,
    pub export_requests: Vec<[u8; e::REQUEST_BYTES]>,
    pub export_responses: Vec<[u8; e::RESPONSE_BYTES]>,
    pub midi: Vec<u8>,
    pub semantic: Vec<u8>,
    pub recovered: [u32; 4],
    pub counts: [u32; 8],
    pub storage: [u32; 6],
    pub frames: u64,
    pub callbacks: u64,
}
impl MediaReport {
    pub fn json(&self) -> String {
        format!("{{\"samples\":{:?},\"requests\":{:?},\"responses\":{:?},\"stagingRequests\":{:?},\"stagingResponses\":{:?},\"exportRequests\":{:?},\"exportResponses\":{:?},\"midiBytes\":{:?},\"semanticBytes\":{:?},\"recovered\":{:?},\"counts\":{:?},\"storage\":{:?},\"frames\":{},\"callbacks\":{}}}",self.samples,self.requests,self.responses,self.staging_requests,self.staging_responses,self.export_requests,self.export_responses,self.midi,self.semantic,self.recovered,self.counts,self.storage,self.frames,self.callbacks)
    }
}
struct Driver {
    host: Host,
    stage: BrowserStagingClient,
    export: BrowserExportClient,
    samples: Vec<[f32; 2]>,
    stage_requests: Vec<[u8; s::REQUEST_BYTES]>,
    stage_responses: Vec<[u8; s::RESPONSE_BYTES]>,
    export_requests: Vec<[u8; e::REQUEST_BYTES]>,
    export_responses: Vec<[u8; e::RESPONSE_BYTES]>,
    frame: u64,
    callbacks: u64,
}
impl Driver {
    fn step(&mut self, frames: u32) {
        let output = self.host.render(frames);
        assert_eq!(output.result, 0);
        assert_eq!(output.heap, allocation_guard::Counts::default());
        assert_eq!(output.frames, self.frame + u64::from(frames));
        for i in frames as usize..128 {
            assert_eq!([output.left[i], output.right[i]], [0.; 2]);
        }
        self.samples
            .extend((0..frames as usize).map(|i| [output.left[i], output.right[i]]));
        self.frame = output.frames;
        self.callbacks = output.callbacks;
    }
    fn stage(
        &mut self,
        kind: u8,
        at: u64,
        args: [u64; 3],
        parts: &[u32],
    ) -> [u8; s::RESPONSE_BYTES] {
        assert_eq!(
            self.stage
                .prepare(kind, 0, 1, at, args[0], args[1], args[2], 0, false),
            0
        );
        let packet = self.stage.request;
        if self.stage_requests.is_empty() {
            let mut bad = packet;
            bad[0] ^= 1;
            assert_eq!(self.host.staging(bad), 2);
            let mut foreign = packet;
            foreign[8] ^= 128;
            assert_eq!(self.host.staging(foreign), 9);
        }
        if kind == s::SELECT {
            for index in [1u64 << 32, u64::MAX] {
                let mut alias = packet;
                e::put64(&mut alias, 48, index);
                assert_eq!(self.host.staging(alias), 7);
            }
        }
        assert_eq!(self.host.staging(packet), 0);
        self.stage_requests.push(packet);
        for &frames in parts {
            self.step(frames);
        }
        for _ in 0..2 {
            let packet = self.host.staging_response().unwrap();
            self.stage.response = packet;
            self.stage.response[8] ^= 128;
            assert_eq!(self.stage.receive(), 9);
            self.stage.response = packet;
            assert_eq!(self.stage.receive(), 0);
            assert_eq!(self.stage.receive(), 8);
            self.stage_responses.push(packet);
        }
        assert!(self.host.staging_response().is_none());
        assert_eq!(self.stage.outstanding(), 0);
        let reply = *self.stage_responses.last().unwrap();
        assert_eq!(e::get32(&reply, 64), s::APPLIED);
        reply
    }
    fn export(&mut self, kind: u8, position: u64, channel: u8) -> [u8; e::RESPONSE_BYTES] {
        assert_eq!(
            self.export.prepare(
                kind,
                0,
                1,
                if matches!(kind, e::AUDIO_READ | e::MIDI_READ) {
                    0
                } else {
                    self.frame
                },
                position,
                channel,
                0,
                false
            ),
            0
        );
        assert_eq!(self.host.export(self.export.request), 0);
        self.export_requests.push(self.export.request);
        self.step(1);
        let count = if matches!(kind, e::AUDIO_READ | e::MIDI_READ) {
            1
        } else {
            2
        };
        for _ in 0..count {
            let packet = self.host.export_response().unwrap();
            self.export.response = packet;
            self.export.response[16] ^= 128;
            assert_eq!(self.export.receive(), 9);
            self.export.response = packet;
            assert_eq!(self.export.receive(), 0);
            assert_eq!(self.export.receive(), 11);
            self.export_responses.push(packet);
        }
        assert!(self.host.export_response().is_none());
        assert_eq!(self.export.outstanding(), 0);
        let response = *self.export_responses.last().unwrap();
        assert_eq!(e::get32(&response, 12), e::APPLIED);
        response
    }
}
pub fn run(threaded: bool, parts: &[u32]) -> MediaReport {
    assert_eq!(parts.iter().sum::<u32>(), 57);
    assert!(parts.iter().all(|n| *n <= 128));
    let mut d = Driver {
        host: Host::new_media(u64::MAX, threaded),
        stage: BrowserStagingClient::new(u64::MAX).unwrap(),
        export: BrowserExportClient::new(u64::MAX).unwrap(),
        samples: vec![],
        stage_requests: vec![],
        stage_responses: vec![],
        export_requests: vec![],
        export_responses: vec![],
        frame: 0,
        callbacks: 0,
    };
    let storage = d.host.storage();
    assert_eq!(storage, [2688, 64, 96, 1024, 2, 1]);
    let mut client = BrowserClient::new(u64::MAX).unwrap();
    let mut requests = vec![];
    for (kind, at, position) in [(2, 0, 0), (4, 6, 1)] {
        assert_eq!(client.prepare(kind, at, position, 0), 0);
        assert_eq!(d.host.primitive(client.request), 0);
        requests.push(client.request);
    }
    let target = 1u64 << 32;
    let source = target | 1;
    d.stage(s::PLAY, 0, [source, 0, 0], &[1]);
    for index in [2, 1u64 << 32, u64::MAX] {
        assert_eq!(
            d.stage
                .prepare(s::SELECT, 0, 1, 4, target, index, 0, 0, false),
            7
        );
        assert_eq!(d.stage.outstanding(), 0);
    }
    d.stage(s::SELECT, 4, [target, 1, 0], &[6]);
    let countdown = d.stage(s::RECORD_AFTER, 7, [target, source, 2], parts);
    assert_eq!(
        (
            e::get64(&countdown, 32),
            e::get64(&countdown, 40),
            e::get64(&countdown, 48),
            e::get64(&countdown, 56),
            e::get64(&countdown, 88)
        ),
        (7, 7, 7, 9, 9)
    );
    let cut = d.stage(s::QUERY_STATE, 64, [target, 0, 0], &[1]);
    assert_eq!(
        [88, 96, 104, 112, 120].map(|at| e::get64(&cut, at)),
        [64, 40, 8, 4, 2]
    );
    let mut responses = vec![];
    while let Some(packet) = d.host.primitive_response() {
        client.response = packet;
        assert_eq!(client.receive(), 0);
        responses.push(packet);
    }
    assert_eq!(responses.len(), 4);
    assert_eq!(client.outstanding(), 0);
    let descriptor = d.export(e::EXPORT, 0, 0);
    assert_eq!(
        (
            e::get64(&descriptor, 104),
            e::get64(&descriptor, 136),
            e::get64(&descriptor, 144)
        ),
        (65, 8, 40)
    );
    assert_eq!(
        [160, 168, 176, 184, 192, 200, 208, 216].map(|at| e::get64(&descriptor, at)),
        [320, 30, 49, 49, 9, 49, 9, 49]
    );
    assert!(matches!(d.host.stop(), Err(15)));
    d.export(e::AUDIO_CURSOR, 0, 0);
    let mut audio = vec![];
    for position in (0..320).step_by(64) {
        let data = d.export(e::AUDIO_READ, position, 0);
        assert_eq!(e::get32(&data, 100), 64);
        audio.extend_from_slice(&data[e::DATA..e::DATA + 64]);
    }
    assert_eq!(
        audio,
        [1f32, 10.]
            .into_iter()
            .cycle()
            .take(80)
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>()
    );
    d.export(e::MIDI_CURSOR, 0, 0);
    let midi = d.export(e::MIDI_READ, 0, 0);
    assert_eq!(e::get32(&midi, 100), 30);
    let mut captured = vec![];
    for key in [62, 70] {
        captured.extend(0u64.to_le_bytes());
        captured.extend(3u32.to_le_bytes());
        captured.extend([0x80, key, 0]);
    }
    assert_eq!(&midi[e::DATA..e::DATA + 30], captured);
    for channel in 0..16 {
        let initial = d.export(e::INITIAL, 0, channel);
        let mut expected = [0; e::INITIAL_BYTES];
        expected[0] = channel;
        expected[2..6].fill(255);
        expected[10..12].copy_from_slice(&8192u16.to_le_bytes());
        for key in 0..128 {
            expected[16 + key * 4 + 2] = 255;
            expected[528 + key] = if [64, 66, 69].contains(&key) { 0 } else { 255 };
        }
        if channel == 0 {
            for (key, velocity) in [(62, 90), (70, 50)] {
                expected[16 + key * 4] = velocity;
                expected[16 + key * 4 + 1] = 1;
            }
        }
        assert_eq!(&initial[e::DATA..e::DATA + e::INITIAL_BYTES], expected);
    }
    d.export(e::RELEASE, 0, 0);
    assert_eq!(d.export.generation(0), 2);
    let counts = [
        client.outstanding(),
        d.stage.outstanding(),
        d.stage.retained(),
        d.export.outstanding(),
        d.export.retained(),
        d.host.counts()[0],
        d.host.counts()[2],
        d.host.staging_counts()[1],
    ];
    assert_eq!(counts, [0; 8]);
    assert_eq!(d.frame, 91);
    let stopped = d.host.stop().unwrap();
    assert_eq!(stopped.recovered, [42, 16, 16, 0]);
    assert_eq!(stopped.journal, expected::midi());
    assert_eq!(stopped.semantic, expected::semantic(91));
    for (frame, sample) in d.samples.iter().enumerate() {
        let v = match frame {
            0 => 3.,
            1 => 6.,
            2 => 9.,
            3 => 12.,
            4 => 11.,
            5 | 6 => 21.,
            7 => 31.,
            8 => 11.,
            _ => 1.,
        };
        assert_eq!(*sample, [v, v * 10.]);
    }
    MediaReport {
        samples: d.samples,
        requests,
        responses,
        staging_requests: d.stage_requests,
        staging_responses: d.stage_responses,
        export_requests: d.export_requests,
        export_responses: d.export_responses,
        midi: stopped.journal,
        semantic: stopped.semantic,
        recovered: stopped.recovered,
        counts: counts.map(|n| n as u32),
        storage,
        frames: d.frame,
        callbacks: d.callbacks,
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn replacement_countdown_prefix_exports_and_bounded_actors_match_complete_oracles() {
        let reference = super::run(false, &[57]);
        for threaded in [false, true] {
            for parts in [vec![57], vec![1; 57], vec![0, 3, 1, 0, 17, 36, 0]] {
                let mut report = super::run(threaded, &parts);
                report.callbacks = reference.callbacks;
                for (response, original) in report
                    .export_responses
                    .iter_mut()
                    .zip(&reference.export_responses)
                {
                    if super::e::get32(response, 8) & 2 != 0 {
                        assert!(super::e::get64(response, 224) > 0);
                        assert!(super::e::get64(response, 232) > 0);
                        response[224..240].copy_from_slice(&original[224..240]);
                    }
                }
                assert_eq!(report, reference);
            }
        }
    }
}
