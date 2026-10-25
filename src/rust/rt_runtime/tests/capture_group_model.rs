mod support;
use rt_runtime::{
    buffer::MidiBuffer, midi_state::MidiState, resource::*, Budgets, Config, Error, Frame, Revision,
};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;
#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
const SEED: u64 = 0x7a49_c83d_502e_19b6;
fn random(seed: &mut u64) -> u64 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *seed >> 32
}
#[derive(Default)]
struct Take {
    start: u64,
    audio: Vec<f32>,
    midi: Vec<(u64, [u8; 3])>,
    initial: u8,
    active: bool,
    incomplete: bool,
}
impl Take {
    fn frames(&self) -> usize {
        self.audio.len() / 2
    }
    fn bytes(&self, member: usize) -> usize {
        if member == 0 {
            self.audio.len() * 4
        } else {
            self.midi.len() * 15
        }
    }
}
struct Lease {
    actual: Snapshot,
    take: usize,
    member: usize,
    frames: usize,
    events: usize,
    revision: u64,
    active: bool,
    incomplete: bool,
    private_tail: bool,
}
struct Pending {
    actual: CaptureIntervalId,
    frames: usize,
    audio: Vec<f32>,
    midi: Vec<(u32, [u8; 3])>,
    ready: [bool; 2],
    pages: usize,
}
#[derive(Default)]
struct Model {
    page_capacity: usize,
    takes: Vec<Take>,
    prepared: Option<usize>,
    current: Option<usize>,
    token: Option<CaptureTakeId>,
    pending: Option<Pending>,
    leases: Vec<Lease>,
    revision: u64,
    at: u64,
    take_sequence: u64,
    interval_sequence: u64,
    failed: bool,
}
impl Model {
    fn recording(&self) -> bool {
        self.current.is_some_and(|i| self.takes[i].active)
    }
    fn live(&self, take: usize, member: usize) -> bool {
        self.current == Some(take)
            || self.prepared == Some(take)
            || self
                .leases
                .iter()
                .any(|l| l.take == take && l.member == member)
    }
    fn usage(&self) -> (usize, usize, usize, usize, usize) {
        let mut versions = 0;
        let mut pages = 0;
        let mut midi = 0;
        for (index, take) in self.takes.iter().enumerate() {
            for member in 0..2 {
                if self.live(index, member) {
                    versions += 1;
                    pages += take.bytes(member).div_ceil(16);
                    if member == 1 {
                        midi += 1;
                    }
                }
            }
        }
        pages += self.leases.iter().filter(|l| l.private_tail).count();
        let slots = self.leases.len() + if self.token.is_some() { 2 } else { 0 };
        (
            8 - versions,
            self.page_capacity - pages,
            8 - slots,
            3 - midi,
            self.pending.as_ref().map_or(0, |p| p.pages),
        )
    }
    fn check(
        &self,
        store: &ResourceStore,
        group: CaptureGroupId,
        resources: [rt_runtime::ResourceId; 2],
    ) {
        let (versions, pages, snapshots, midi, reserved) = self.usage();
        let actual = store.usage();
        assert_eq!(actual.free_versions, versions);
        assert_eq!(actual.free_pages, pages);
        assert_eq!(actual.free_snapshots, snapshots);
        assert_eq!(actual.reserved_pages, reserved);
        assert_eq!(actual.retired_pages, 0);
        assert_eq!(actual.retired_versions, 0);
        assert_eq!(store.midi_index_usage().free_versions, midi);
        let phase = if self.pending.is_some() {
            CapturePhase::Reserved
        } else if self.recording() {
            CapturePhase::Recording
        } else if self.prepared.is_some() {
            CapturePhase::Prepared
        } else {
            CapturePhase::Idle
        };
        let info = store.capture_group_info(group).unwrap();
        assert_eq!(info.phase, phase);
        assert_eq!(info.take_sequence, self.take_sequence);
        assert_eq!(info.interval_sequence, self.interval_sequence);
        assert_eq!(
            info.interval_frames,
            self.pending.as_ref().map_or(0, |p| p.frames)
        );
        for (member, resource) in resources.into_iter().enumerate() {
            assert_eq!(store.revision(resource), Ok(Revision(self.revision)));
            if let Some(index) = self.current {
                let take = &self.takes[index];
                let info = store.info(resource).unwrap();
                assert_eq!(info.revision, Revision(self.revision));
                assert_eq!(info.description.bytes, take.bytes(member));
                assert_eq!(info.description.frames, take.frames() as u64);
                assert_eq!(
                    info.capture.unwrap(),
                    CaptureInfo {
                        start_frame: Frame(take.start),
                        next_frame: Frame(take.start + take.frames() as u64),
                        active: take.active,
                        incomplete: take.incomplete.then_some(Error::Capacity)
                    }
                );
            } else {
                assert_eq!(store.info(resource), Err(Error::InvalidState));
            }
        }
    }
    fn add_lease(&mut self, actual: Snapshot, member: usize, private_tail: bool) {
        let index = self.current.unwrap();
        let take = &self.takes[index];
        self.leases.push(Lease {
            actual,
            take: index,
            member,
            frames: take.frames(),
            events: take.midi.len(),
            revision: self.revision,
            active: take.active,
            incomplete: take.incomplete,
            private_tail,
        });
    }
    fn verify_leases(&self, store: &mut ResourceStore) {
        for lease in &self.leases {
            let take = &self.takes[lease.take];
            let mut bytes = vec![0; lease.frames * 8];
            assert_no_heap(|| {
                assert_eq!(lease.actual.description.frames, lease.frames as u64);
                assert_eq!(
                    lease.actual.cut_frame,
                    Frame(take.start + lease.frames as u64)
                );
                assert_eq!(lease.actual.revision, Revision(lease.revision));
                assert_eq!(lease.actual.capture.unwrap().active, lease.active);
                assert_eq!(
                    lease.actual.capture.unwrap().incomplete,
                    lease.incomplete.then_some(Error::Capacity)
                );
                let current = self.current == Some(lease.take)
                    && self.revision == lease.revision
                    && take.active == lease.active
                    && take.incomplete == lease.incomplete;
                assert_eq!(
                    store.currentness(lease.actual.id),
                    Ok(if current {
                        Currentness::Current
                    } else {
                        Currentness::Stale
                    })
                );
                let index = store.read_index(lease.actual.id).unwrap();
                if lease.member == 0 {
                    assert_eq!(lease.actual.description.bytes, bytes.len());
                    let mut cursor = store.render_io().cursor_at(index, 0).unwrap();
                    let mut written = 0;
                    while written < bytes.len() {
                        let count = store
                            .render_io()
                            .read(&mut cursor, &mut bytes[written..])
                            .unwrap();
                        assert!(count > 0);
                        written += count;
                    }
                    for (actual, expected) in
                        bytes.chunks_exact(4).zip(&take.audio[..lease.frames * 2])
                    {
                        assert_eq!(actual, &expected.to_le_bytes());
                    }
                } else {
                    assert_eq!(lease.actual.description.bytes, lease.events * 15);
                    let mut cursor = store.render_io().midi_cursor(index).unwrap();
                    for &(frame, expected) in &take.midi[..lease.events] {
                        let event = store.render_io().midi_next(&mut cursor).unwrap().unwrap();
                        assert_eq!(event.frame(), frame);
                        let mut packet = [0; 3];
                        assert_eq!(store.render_io().midi_read(event, &mut packet), Ok(3));
                        assert_eq!(packet, expected);
                    }
                    assert_eq!(store.render_io().midi_next(&mut cursor), Ok(None));
                    for frame in 0..=lease.frames as u64 {
                        let expected = take.midi[..lease.events]
                            .iter()
                            .rev()
                            .find(|(at, _)| *at < frame)
                            .map_or(take.initial, |(_, packet)| packet[2]);
                        let position = store.render_io().midi_seek(index, frame).unwrap();
                        assert_eq!(
                            position
                                .state
                                .channel(0)
                                .unwrap()
                                .note(60)
                                .unwrap()
                                .velocity(),
                            expected
                        );
                        assert!(position.replayed_events < 4);
                    }
                }
            });
        }
    }
}
fn drain(store: &mut ResourceStore) {
    assert_no_heap(|| {
        for _ in 0..80 {
            if store.usage().retired_pages == 0 && store.usage().retired_versions == 0 {
                return;
            }
            store.begin_callback();
            store.reclaim();
        }
        panic!("retirement exceeded the prepared page/version bound");
    });
}
#[shoop_test]
fn generated_capture_ownership_matches_independent_pool_and_content_model() {
    check_model(64);
    check_model(16);
}
fn check_model(page_capacity: usize) {
    let cfg = Config {
        max_frames: 4,
        resource_slots: 8,
        resource_pages: page_capacity,
        page_bytes: 16,
        midi_events_per_port: 4,
        midi_bytes_per_port: 12,
        budgets: Budgets {
            transfer_bytes: 16,
            validation_steps: 1,
            ..Budgets::default()
        },
        ..Config::default()
    }
    .prepare()
    .unwrap();
    let mut store = ResourceStore::with_capture_groups(
        cfg,
        Some(MidiIndexConfig {
            versions: 3,
            checkpoints_per_version: 8,
            replay_events: 4,
        }),
        CaptureGroupConfig {
            groups: 1,
            members_per_group: 2,
            staging_bytes_per_group: 92,
        },
    )
    .unwrap();
    let resources = [store.create().unwrap(), store.create().unwrap()];
    let group = store
        .bind_capture_group(&[
            CaptureMember {
                resource: resources[0],
                format: Format::Audio { channels: 2 },
                midi_events: 0,
                midi_bytes: 0,
            },
            CaptureMember {
                resource: resources[1],
                format: Format::Midi,
                midi_events: 4,
                midi_bytes: 12,
            },
        ])
        .unwrap();
    let mut model = Model {
        page_capacity,
        ..Model::default()
    };
    let mut seed = SEED;
    let mut input = MidiBuffer::new(4, 12).unwrap();
    let mut coverage = [0usize; 13];
    for step in 0..1024 {
        assert_no_heap(|| store.begin_callback());
        let choice = random(&mut seed) % 8;
        if choice == 0 && !model.leases.is_empty() {
            let index = random(&mut seed) as usize % model.leases.len();
            let lease = model.leases.swap_remove(index);
            assert_no_heap(|| store.release_snapshot(lease.actual.id)).unwrap();
            coverage[0] += 1;
        } else if choice == 1 && model.current.is_some() {
            let member = (random(&mut seed) % 2) as usize;
            let take = &model.takes[model.current.unwrap()];
            let private = take.active && take.bytes(member) % 16 != 0;
            let (_, pages, snapshots, _, reserved) = model.usage();
            let expected = if snapshots == 0 || (private && pages == reserved) {
                Err(Error::Capacity)
            } else {
                Ok(())
            };
            let result = assert_no_heap(|| store.snapshot(resources[member], Frame(model.at)));
            assert_eq!(result.as_ref().map(|_| ()).map_err(|e| *e), expected);
            if let Ok(snapshot) = result {
                model.add_lease(snapshot, member, private);
                coverage[1] += 1;
            }
        } else if model.prepared.is_some() {
            if choice == 2 {
                assert_no_heap(|| store.cancel_capture_take(model.token.unwrap())).unwrap();
                model.prepared = None;
                model.token = None;
                coverage[2] += 1;
            } else {
                assert_no_heap(|| {
                    store.activate_capture_take(model.token.unwrap(), Frame(model.at))
                })
                .unwrap();
                model.current = model.prepared.take();
                model.revision += 1;
                let take = &mut model.takes[model.current.unwrap()];
                take.start = model.at;
                take.active = true;
                model.failed = false;
                coverage[3] += 1;
            }
        } else if model.pending.is_some() {
            if choice == 2 {
                assert_no_heap(|| {
                    store.cancel_capture_interval(model.pending.as_ref().unwrap().actual)
                })
                .unwrap();
                model.pending = None;
                coverage[4] += 1;
            } else {
                let pending = model.pending.as_mut().unwrap();
                if !pending.ready[0] {
                    assert_no_heap(|| store.stage_capture_audio(pending.actual, 0, &pending.audio))
                        .unwrap();
                    pending.ready[0] = true;
                    coverage[5] += 1;
                } else if !pending.ready[1] {
                    input.clear();
                    for &(offset, packet) in &pending.midi {
                        input.push(offset, pending.frames, &packet).unwrap();
                    }
                    assert_no_heap(|| store.stage_capture_midi(pending.actual, 1, &input)).unwrap();
                    pending.ready[1] = true;
                    coverage[6] += 1;
                } else {
                    assert_no_heap(|| store.commit_capture_interval(pending.actual)).unwrap();
                    let pending = model.pending.take().unwrap();
                    let take = &mut model.takes[model.current.unwrap()];
                    let base = take.frames() as u64;
                    take.audio.extend(pending.audio);
                    take.midi.extend(
                        pending
                            .midi
                            .into_iter()
                            .map(|(offset, packet)| (base + u64::from(offset), packet)),
                    );
                    model.at += pending.frames as u64;
                    model.revision += 1;
                    model.failed = false;
                    coverage[7] += 1;
                }
            }
        } else if model.recording() {
            if choice == 2 || model.failed {
                let mut output = [None; 2];
                assert_no_heap(|| {
                    store.freeze_capture_take(
                        model.token.unwrap(),
                        Frame(model.at),
                        model.failed.then_some(Error::Capacity),
                        &mut output,
                    )
                })
                .unwrap();
                let take = &mut model.takes[model.current.unwrap()];
                take.active = false;
                take.incomplete = model.failed;
                model.token = None;
                for (member, snapshot) in output.into_iter().enumerate() {
                    model.add_lease(snapshot.unwrap(), member, false);
                }
                coverage[8] += 1;
            } else {
                let frames = (random(&mut seed) % 4 + 1) as usize;
                let take = &model.takes[model.current.unwrap()];
                let pages = (take.bytes(0) + frames * 8).div_ceil(16) - take.bytes(0).div_ceil(16)
                    + (take.bytes(1) + 60).div_ceil(16)
                    - take.bytes(1).div_ceil(16);
                let (_, free, _, _, reserved) = model.usage();
                let expected = if pages > free - reserved || (take.midi.len() + 4) / 4 + 1 > 8 {
                    Err(Error::Capacity)
                } else {
                    Ok(())
                };
                let result = assert_no_heap(|| {
                    store.reserve_capture_interval(model.token.unwrap(), Frame(model.at), frames)
                });
                assert_eq!(result.as_ref().map(|_| ()).map_err(|e| *e), expected);
                if let Ok(interval) = result {
                    let mut audio = Vec::new();
                    for _ in 0..frames {
                        let value = (random(&mut seed) % 32768) as f32;
                        audio.extend([value, -value]);
                    }
                    let count = random(&mut seed) % 3;
                    let midi = (0..count)
                        .map(|i| {
                            (
                                if i == 0 { 0 } else { frames as u32 - 1 },
                                [0x90, 60, (random(&mut seed) % 127 + 1) as u8],
                            )
                        })
                        .collect();
                    model.pending = Some(Pending {
                        actual: interval,
                        frames,
                        audio,
                        midi,
                        ready: [false; 2],
                        pages,
                    });
                    model.interval_sequence += 1;
                    coverage[9] += 1;
                } else {
                    model.failed = true;
                    coverage[10] += 1;
                }
            }
        } else {
            let (versions, _, snapshots, midi, _) = model.usage();
            let expected = if versions < 2 || snapshots < 2 || midi == 0 {
                Err(Error::Capacity)
            } else {
                Ok(())
            };
            let initial = (random(&mut seed) % 127 + 1) as u8;
            let mut state = MidiState::default();
            state.apply_packet(&[0x90, 60, initial], 3).unwrap();
            let result = assert_no_heap(|| store.prepare_capture_take(group, &[state]));
            assert_eq!(result.as_ref().map(|_| ()).map_err(|e| *e), expected);
            if let Ok(token) = result {
                model.prepared = Some(model.takes.len());
                model.takes.push(Take {
                    initial,
                    ..Take::default()
                });
                model.token = Some(token);
                model.take_sequence += 1;
                coverage[11] += 1;
            } else {
                coverage[12] += 1;
            }
        }
        drain(&mut store);
        model.check(&store, group, resources);
        if step % 32 == 0 {
            model.verify_leases(&mut store);
        }
    }
    model.verify_leases(&mut store);
    eprintln!("capture model seed={SEED:#x}, pages={page_capacity}, steps=1024, action counts={coverage:?}");
    if page_capacity == 16 {
        assert!(
            coverage[10] > 0,
            "small-pool reservation refusal not visited"
        );
    }
    for (action, count) in coverage.into_iter().enumerate() {
        if action != 10 {
            assert!(count > 0, "unvisited model action {action}");
        }
    }
    if let Some(pending) = model.pending.take() {
        assert_no_heap(|| store.cancel_capture_interval(pending.actual)).unwrap();
    }
    if model.prepared.is_some() {
        assert_no_heap(|| store.cancel_capture_take(model.token.unwrap())).unwrap();
    } else if model.recording() {
        let mut output = [None; 2];
        assert_no_heap(|| {
            store.freeze_capture_take(model.token.unwrap(), Frame(model.at), None, &mut output)
        })
        .unwrap();
        for snapshot in output.into_iter().flatten() {
            assert_no_heap(|| store.release_snapshot(snapshot.id)).unwrap();
        }
    }
    for lease in model.leases {
        assert_no_heap(|| store.release_snapshot(lease.actual.id)).unwrap();
    }
    assert_no_heap(|| {
        store.release_capture_group(group).unwrap();
        for resource in resources {
            store.remove(resource).unwrap();
        }
    });
    drain(&mut store);
    assert_eq!(store.usage().free_pages, page_capacity);
    assert_eq!(store.usage().free_versions, 8);
    assert_eq!(store.usage().free_snapshots, 8);
    assert_eq!(store.usage().reserved_pages, 0);
    assert_eq!(store.midi_index_usage().free_versions, 3);
}
