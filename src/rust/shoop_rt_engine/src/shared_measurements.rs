use super::*;
use std::{fmt::Write, fs, path::PathBuf};

pub(super) struct Callback {
    pub frame: u64,
    pub frames: usize,
    pub nanos: u128,
    pub work: (resource::Work, resource::RenderWork),
    pub usage: resource::PoolUsage,
    pub ticks: usize,
    pub phase: &'static str,
    pub midi_overflows: usize,
}

#[shoop_test]
#[ignore = "CI release measurement with retained raw callback artifacts"]
fn fixed_shared_callback_measurements() {
    let directory =
        PathBuf::from(std::env::var("SHOOP_SHARED_REPORT_DIR").expect("report directory"));
    fs::create_dir_all(&directory).unwrap();
    let source = std::env::var("SHOOP_SHARED_SOURCE_SHA").expect("source SHA");
    let mut summary = std::format!("source={source}\nrate=48000 max_host=128 sources=1 loops=2 channels=2 arena_pages=12288 page_bytes=256 audio_pages=1024 midi_pages=8192 candidate_slots=2 preparation_steps_per_callback=8\nTiming covers the complete guarded Render::process call, including all fixed ticks and callback-end maintenance. Input generation and control collection are outside the interval. Every sample and miss is retained. CI timings are observations, not device acceptance.\n");
    for q in [1, 16, 32, 64, 128] {
        for partition in [1, 7, 31, 128] {
            let mut h = Fixture::new_with_rate(q, partition, 48000);
            let Reply::SharedCapacity(capacity) = h.op(Operation::SharedCapacity) else {
                panic!("capacity")
            };
            assert_eq!((capacity.sources, capacity.candidates), (1, 2));
            assert_eq!(capacity.arena_payload_bytes, 12288 * 256);
            assert_eq!(capacity.reserved_payload_bytes, (1024 + 8192) * 256);
            assert!(capacity.coordinator_bytes > q * 2 * 4);
            assert!(capacity.store_metadata_bytes >= capacity.midi_state_bytes);
            h.timings = Some(Vec::new());
            let (mut current, sibling, _) = h.grabs();
            h.adopt(0, current);
            h.adopt(1, sibling);
            let old = h.export(0);
            h.measure_phase = "mixed_playback";
            for slot in 0..2 {
                h.op(Operation::Transition {
                    target: target(slot),
                    timing: Timing::At,
                    transition: Transition::Play {
                        position: 0,
                        retrigger: false,
                    },
                });
            }
            let patch = h.snapshot(sibling.midi.unwrap());
            for round in 0..4 {
                h.measure_phase = "private_audio";
                let id = h.audio_candidate(current);
                h.measure_phase = "midi_build";
                h.op(Operation::CandidateMidi {
                    id,
                    position: current.frames - 1,
                    input: patch.id,
                });
                let mut ready = false;
                for _ in 0..128 {
                    let Reply::SharedWork(work) = h.op(Operation::SharedWork) else {
                        panic!("work")
                    };
                    assert!(work.edit_steps <= 8 && work.jobs_scanned <= 16);
                    if matches!(
                        h.op(Operation::CandidateStatus { id }),
                        Reply::Candidate {
                            status: CandidateStatus::Ready,
                            ..
                        }
                    ) {
                        ready = true;
                        break;
                    }
                }
                assert!(ready);
                h.measure_phase = "publication";
                let Reply::CandidatePublished { take, .. } =
                    h.op(Operation::CandidatePublish { id })
                else {
                    panic!("publish round {round}")
                };
                for resource in [current.audio, current.midi].into_iter().flatten() {
                    h.resource(ResourceRequest::Remove { resource });
                }
                current = take;
                h.measure_phase = "mixed_playback";
                let end = h.aligned_future(128 + current.frames * 3);
                h.run_to(end);
            }
            h.resource(ResourceRequest::ReleaseSnapshot { snapshot: patch.id });
            let data = h.timings.take().unwrap();
            let mut stopped = h.engine.render.into_stopped();
            let (_, midi) = stopped.planner.shared_stores(0).unwrap();
            let midi_work = stopped.resources.midi_store_work(midi.unwrap()).unwrap();
            assert!(!midi_work.counter_overflow);
            assert!(!bytes(&mut stopped.resources, old.view.audio.unwrap().snapshot).is_empty());
            let mut times: Vec<_> = data.iter().map(|row| row.nanos).collect();
            times.sort_unstable();
            let misses = data
                .iter()
                .filter(|row| row.nanos * 48000 > row.frames as u128 * 1_000_000_000)
                .count();
            let total: u128 = times.iter().sum();
            let midi_overflows: usize = data.iter().map(|row| row.midi_overflows).sum();
            writeln!(summary, "Q={q} partition={partition} callbacks={} mean_ns={} p50_ns={} p95_ns={} p99_ns={} max_ns={} misses={misses} midi_overflows={midi_overflows} capacity={capacity:?} midi={midi_work:?}",
                data.len(), total / times.len() as u128, times[times.len()/2], times[(times.len()-1)*95/100], times[(times.len()-1)*99/100], times[times.len()-1]).unwrap();
            let mut raw = std::string::String::from("frame,frames,phase,ticks,nanoseconds,deadline_ns,miss,render_operations,render_copied_bytes,render_validation_steps,background_copied_bytes,background_validation_steps,reclaimed_pages,reclaimed_versions,free_pages,reserved_pages,free_versions,free_snapshots,retired_versions,retired_pages,midi_overflows\n");
            for row in data {
                assert!(!row.work.1.counter_overflow);
                writeln!(
                    raw,
                    "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                    row.frame,
                    row.frames,
                    row.phase,
                    row.ticks,
                    row.nanos,
                    row.frames as u128 * 1_000_000_000 / 48000,
                    u8::from(row.nanos * 48000 > row.frames as u128 * 1_000_000_000),
                    row.work.1.operations,
                    row.work.1.copied_bytes,
                    row.work.1.validation_steps,
                    row.work.0.copied_bytes,
                    row.work.0.validation_steps,
                    row.work.0.reclaimed_pages,
                    row.work.0.reclaimed_versions,
                    row.usage.free_pages,
                    row.usage.reserved_pages,
                    row.usage.free_versions,
                    row.usage.free_snapshots,
                    row.usage.retired_versions,
                    row.usage.retired_pages,
                    row.midi_overflows
                )
                .unwrap();
            }
            fs::write(
                directory.join(std::format!("q{q}-host{partition}.csv")),
                raw,
            )
            .unwrap();
            stopped.cleanup().unwrap();
            assert_eq!(stopped.resource_usage().free_pages, 12288);
            fs::write(directory.join("summary.txt"), &summary).unwrap();
        }
    }
}
