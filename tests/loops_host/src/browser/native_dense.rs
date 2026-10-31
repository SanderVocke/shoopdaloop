use super::{native_graph::transport::Host, BrowserClient};
use crate::{
    allocation_guard,
    export_wire::{get32, get64},
};

mod expected;

#[derive(Debug, PartialEq)]
pub struct DenseReport {
    pub samples: Vec<[f32; 2]>,
    pub requests: Vec<Vec<u8>>,
    pub responses: Vec<Vec<u8>>,
    pub midi: Vec<u8>,
    pub semantic: Vec<u8>,
    pub recovered: [u32; 4],
    pub counts: [u32; 4],
}
impl DenseReport {
    pub fn json(&self) -> String {
        format!("{{\"samples\":{:?},\"requests\":{:?},\"responses\":{:?},\"midiBytes\":{:?},\"semanticBytes\":{:?},\"recovered\":{:?},\"counts\":{:?}}}",self.samples,self.requests,self.responses,self.midi,self.semantic,self.recovered,self.counts)
    }
}
pub fn run(threaded: bool, partitions: &[u32]) -> DenseReport {
    run_mode(threaded, partitions, false)
}
pub fn run_stalls(threaded: bool, partitions: &[u32]) -> DenseReport {
    run_mode(threaded, partitions, true)
}
fn run_mode(threaded: bool, partitions: &[u32], stalled: bool) -> DenseReport {
    assert_eq!(partitions.iter().sum::<u32>(), 128);
    assert!(partitions.iter().all(|&n| n <= 128));
    let mut host = Host::new_dense(u64::MAX, threaded);
    let mut client = BrowserClient::new(u64::MAX).unwrap();
    let mut requests = Vec::new();
    for (kind, at) in [(1, 0), (2, 1), (4, 64), (4, 130)] {
        assert_eq!(client.prepare(kind, at, 0, 0), 0);
        assert_eq!(host.primitive(client.request), 0);
        requests.push(client.request.to_vec());
    }
    assert_eq!(client.prepare(3, 0, 0, 0), 10);
    let mut samples = Vec::new();
    for &frames in partitions.iter().chain(std::iter::once(&1)) {
        let rendered = host.render(frames);
        assert_eq!(rendered.result, 0);
        assert_eq!(rendered.heap, allocation_guard::Counts::default());
        samples.extend((0..frames as usize).map(|i| [rendered.left[i], rendered.right[i]]));
        assert_eq!(host.counts(), [4, 0, 0]);
    }
    assert_eq!(host.stop().err(), Some(15));
    assert_eq!(client.prepare(3, 129, 0, 4), 0);
    assert_eq!(host.primitive(client.request), 0);
    requests.push(client.request.to_vec());
    let rendered = host.render(2);
    assert_eq!(rendered.result, 0);
    assert_eq!(rendered.frames, 131);
    assert_eq!(rendered.heap, allocation_guard::Counts::default());
    samples.extend((0..2).map(|i| [rendered.left[i], rendered.right[i]]));
    assert_eq!(host.counts(), [5, 0, 0]);
    let mut responses = Vec::new();
    while let Some(response) = host.primitive_response() {
        client.response = response;
        assert_eq!(client.receive(), 0);
        assert_eq!(client.receive(), 11);
        responses.push(response.to_vec());
    }
    assert_eq!(responses.len(), 10);
    if stalled {
        for (kind, at) in [(2, 131), (3, 135)] {
            assert_eq!(client.prepare(kind, at, 0, 0), 0);
            assert_eq!(host.primitive(client.request), 0);
            requests.push(client.request.to_vec());
        }
        let rendered = host.render(5);
        assert_eq!(rendered.result, 0);
        assert_eq!(rendered.frames, 136);
        assert_eq!(rendered.heap, allocation_guard::Counts::default());
        samples.extend((0..5).map(|i| [rendered.left[i], rendered.right[i]]));
        assert_eq!(host.counts(), [2, 0, 0]);
        assert_eq!(client.outstanding(), 2);
        assert_eq!(host.stop().err(), Some(15));
        while let Some(response) = host.primitive_response() {
            client.response = response;
            assert_eq!(client.receive(), 0);
            assert_eq!(client.receive(), 11);
            responses.push(response.to_vec());
        }
        assert_eq!(responses.len(), 14);
    }
    let counts = [
        client.outstanding(),
        host.counts()[0],
        host.counts()[1],
        host.counts()[2],
    ];
    assert_eq!(counts, [0; 4]);
    let stopped = host.stop().unwrap();
    assert_eq!(stopped.recovered, [256, 16, 16, 0]);
    let report = DenseReport {
        samples,
        requests,
        responses,
        midi: stopped.journal,
        semantic: stopped.semantic,
        recovered: stopped.recovered,
        counts,
    };
    check(&report, stalled);
    report
}
fn check(report: &DenseReport, stalled: bool) {
    for (index, response) in report.responses.iter().enumerate() {
        let command = index / 2;
        let at = [0, 1, 64, 130, 129, 131, 135][command];
        let execution = index % 2 == 1;
        assert_eq!(response[6], if execution { 2 } else { 1 });
        assert_eq!(response[7], u8::from(execution));
        assert_eq!(get64(response, 8), u64::MAX);
        assert_eq!(get64(response, 16), command as u64 + 1);
        assert_eq!(get32(response, 24), [0, 1, 2, 3, 4, 0, 1][command]);
        assert_eq!(
            get32(response, 28),
            if !execution {
                1
            } else if command == 3 {
                6
            } else {
                4
            }
        );
        assert_eq!(get64(response, 32), command as u64 + 1);
        assert_eq!(get64(response, 40), at);
        assert_eq!(get64(response, 48), at);
        assert_eq!(get64(response, 56), if execution { at } else { 0 });
        assert_eq!(
            get64(response, 64),
            if execution || command == 4 {
                at
            } else if command > 4 {
                131
            } else {
                0
            }
        );
        assert_eq!(
            get32(response, 72),
            if execution && command == 3 { 261 } else { 0 }
        );
        assert!(response[76..].iter().all(|&byte| byte == 0));
    }
    assert_eq!(
        report.samples,
        (0..if stalled { 136 } else { 131 })
            .map(|frame| if frame == 0 {
                [1., 10.]
            } else if frame < 129 || (131..135).contains(&frame) {
                [0.5, 5.]
            } else {
                [0., 0.]
            })
            .collect::<Vec<_>>()
    );
    let mut expected = Vec::new();
    let mut packet = |frame: u64, bytes: [u8; 3]| {
        expected.extend_from_slice(&frame.to_le_bytes());
        expected.extend_from_slice(&3u32.to_le_bytes());
        expected.extend_from_slice(&bytes);
    };
    packet(0, [0x90, 60, 100]);
    for frame in 1..129 {
        if frame > 1 {
            packet(frame, [0x80, 60, 0]);
        }
        for channel in 0..16 {
            packet(frame, [0xe0 | channel, 0, 64]);
            for cc in [64, 69, 66] {
                packet(frame, [0xb0 | channel, cc, 0]);
            }
        }
        packet(frame, [0x90, 60, 100]);
    }
    packet(129, [0x80, 60, 0]);
    if stalled {
        for frame in 131..135 {
            if frame > 131 {
                packet(frame, [0x80, 60, 0]);
            }
            for channel in 0..16 {
                packet(frame, [0xe0 | channel, 0, 64]);
                for cc in [64, 69, 66] {
                    packet(frame, [0xb0 | channel, cc, 0]);
                }
            }
            packet(frame, [0x90, 60, 100]);
        }
        packet(135, [0x80, 60, 0]);
    }
    assert_eq!(report.midi, expected);
    assert_eq!(report.semantic, expected::semantic(stalled));
    assert_eq!(&report.semantic[16..40], &[0; 24]);
    assert_eq!(
        report.semantic.len(),
        40 + get32(&report.semantic, 8) as usize * 160
    );
    let events = report.semantic[40..]
        .chunks_exact(160)
        .filter(|r| r[0] == 3)
        .map(|r| (get64(r, 8), r[1], get32(r, 16)))
        .collect::<std::collections::BTreeSet<_>>();
    for target in 0..5 {
        assert!(events.contains(&(0, 10, target)));
    }
    for frame in 1..131 {
        assert!(events.contains(&(frame, 10, 1)));
    }
    for frame in 1..129 {
        assert_eq!(events.contains(&(frame, 9, 0)), frame != 1 && frame != 64);
    }
    assert!(!events.contains(&(129, 10, 0)));
}
#[cfg(test)]
mod tests {
    #[test]
    fn acknowledged_restart_keeps_media_and_owning_results_while_transport_is_unpolled() {
        let expected = super::run_stalls(false, &[128]);
        for threaded in [false, true] {
            for partitions in [vec![128], vec![1; 128], [0, 7, 9, 0, 5, 11].repeat(4)] {
                assert_eq!(super::run_stalls(threaded, &partitions), expected);
            }
        }
    }
    #[test]
    fn dense_wire_and_capacity_one_actors_preserve_music_with_all_results_stalled() {
        let expected = super::run(false, &[128]);
        for threaded in [false, true] {
            for partitions in [vec![128], vec![1; 128], [0, 7, 9, 0, 5, 11].repeat(4)] {
                assert_eq!(super::run(threaded, &partitions), expected);
            }
        }
    }
}
