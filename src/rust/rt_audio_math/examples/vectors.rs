use rt_audio_math::{Ramp, Sample, Q824};

fn main() {
    let frames: usize = std::env::args()
        .nth(1)
        .expect("frame count")
        .parse()
        .expect("integer frames");
    let one = Q824::ONE.0;
    let settings = [
        (0, one, 0, 0),
        (0, one, 1, 0),
        (0, one, 7, 0),
        (one, -one, 17, 15),
        (i32::MIN, i32::MAX, 33, 0),
        (i32::MAX, i32::MIN, 7, 0),
        (i32::MIN, i32::MAX, u32::MAX, 1234567890),
        (i32::MAX, i32::MIN, u32::MAX, u32::MAX - 16),
        (0, -one / 2, 0, 0),
        (0, i32::MIN, 0, 0),
        (i32::MIN, i32::MAX, 0, u32::MAX),
        (one, -one, 7, u32::MAX),
    ];
    let edges = [i32::MIN, i32::MAX, -3, -1, 0, 1, 3, one, -one, 123456789];
    for (case, (start, target, duration, elapsed)) in settings.into_iter().enumerate() {
        let ramp = Ramp {
            start: Q824(start),
            target: Q824(target),
            duration,
            elapsed,
        };
        let first: Vec<_> = (0..frames)
            .map(|i| edges[(i + case) % edges.len()])
            .collect();
        let second: Vec<_> = (0..frames)
            .map(|i| edges[(i + case + 3) % edges.len()])
            .collect();
        let expected: Vec<_> = first
            .iter()
            .zip(&second)
            .enumerate()
            .map(|(index, (&a, &b))| Q824(a).multiply(ramp.at(index as u32)).mix(Q824(b)).0)
            .collect();
        println!("{{\"start\":{start},\"target\":{target},\"duration\":{duration},\"elapsed\":{elapsed},\"first\":{first:?},\"second\":{second:?},\"expected\":{expected:?}}}");
    }
}
