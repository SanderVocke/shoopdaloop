#[path = "../../rt_runtime/tests/support/mod.rs"]
mod support;
use rt_runtime::{
    midi_state::MidiState,
    resource::{self, Format, MidiIndexConfig, ResourceStore},
    Budgets, Config, Error, Frame, ResourceId, Revision,
};
use shoop_loop_engine::{media::*, LoopId};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;
#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
fn owner(generation: u32) -> LoopId {
    LoopId::from_parts(0, generation).unwrap()
}
fn store() -> ResourceStore {
    ResourceStore::with_midi_indexing(
        Config {
            resource_slots: 16,
            resource_pages: 128,
            page_bytes: 16,
            budgets: Budgets {
                transfer_bytes: 64,
                validation_steps: 128,
                ..Budgets::default()
            },
            ..Config::default()
        }
        .prepare()
        .unwrap(),
        MidiIndexConfig {
            versions: 16,
            checkpoints_per_version: 2,
            replay_events: 16,
        },
    )
    .unwrap()
}
fn audio_bytes(base: f32) -> Vec<u8> {
    (0..8)
        .flat_map(|n| (base + n as f32).to_le_bytes())
        .collect()
}
fn midi_bytes() -> Vec<u8> {
    [
        0u64.to_le_bytes().as_slice(),
        3u32.to_le_bytes().as_slice(),
        &[0x90, 60, 80],
    ]
    .concat()
}
fn upload(store: &mut ResourceStore, format: Format, bytes: &[u8], indexed: bool) -> ResourceId {
    let id = store.create().unwrap();
    let description = resource::Description {
        format,
        frames: 4,
        bytes: bytes.len(),
    };
    let token = if indexed {
        store
            .begin_indexed_midi_upload(id, Revision(0), description, &MidiState::default())
            .unwrap()
    } else {
        store.begin_upload(id, Revision(0), description).unwrap()
    };
    for (page, chunk) in bytes.chunks(16).enumerate() {
        store.begin_callback();
        store.write_upload(token, page * 16, chunk).unwrap();
    }
    store.finish_upload(token).unwrap();
    store.commit(token).unwrap();
    id
}
fn pair(store: &mut ResourceStore, base: f32) -> [ResourceId; 2] {
    [
        upload(
            store,
            Format::Audio { channels: 2 },
            &audio_bytes(base),
            false,
        ),
        upload(store, Format::Midi, &midi_bytes(), true),
    ]
}
fn input(
    store: &mut ResourceStore,
    ids: [ResourceId; 2],
    who: LoopId,
    revision: u64,
    cut: Frame,
) -> Input {
    let mut source = |id| {
        let snapshot = store.snapshot(id, cut).unwrap();
        Source {
            snapshot,
            index: store.read_index(snapshot.id).unwrap(),
        }
    };
    Input {
        description: Description {
            owner: who,
            revision: Revision(revision),
            frames: 4,
            channels: 2,
            midi: true,
        },
        audio: Some(source(ids[0])),
        midi: Some(source(ids[1])),
    }
}
fn copy(view: View) -> Input {
    Input {
        description: view.description,
        audio: view.audio,
        midi: view.midi,
    }
}
fn verify(store: &mut ResourceStore, view: View, base: f32) {
    let mut bytes = [0; 32];
    for (source, midi) in [(view.audio.unwrap(), false), (view.midi.unwrap(), true)] {
        let mut cursor = store.render_io().cursor_at(source.index, 0).unwrap();
        let end = source.snapshot.description.bytes;
        let mut offset = 0;
        while offset < end {
            let count = store
                .render_io()
                .read(&mut cursor, &mut bytes[offset..end])
                .unwrap();
            assert!(count > 0);
            offset += count;
        }
        if midi {
            assert_eq!(&bytes[..8], &0u64.to_le_bytes());
            assert_eq!(&bytes[8..12], &3u32.to_le_bytes());
            assert_eq!(&bytes[12..15], &[0x90, 60, 80]);
        } else {
            for (n, chunk) in bytes.chunks_exact(4).enumerate() {
                assert_eq!(chunk, (base + n as f32).to_le_bytes());
            }
        }
    }
}
fn release_input(store: &mut ResourceStore, input: Input) {
    for source in [input.audio, input.midi].into_iter().flatten() {
        store.release_snapshot(source.snapshot.id).unwrap();
    }
}
fn cleanup(store: &mut ResourceStore, ids: &[ResourceId]) {
    assert_no_heap(|| {
        for id in ids {
            store.remove(*id).unwrap();
        }
        for _ in 0..160 {
            store.begin_callback();
            store.reclaim();
        }
        assert_eq!(store.usage().free_pages, 128);
        assert_eq!(store.usage().free_versions, 16);
        assert_eq!(store.usage().free_snapshots, 16);
    });
}

#[shoop_test]
fn owner_batch_activation_preserves_old_exports_pins_and_unpublished_reservations() {
    let mut store = store();
    let a = pair(&mut store, 1.);
    let b = pair(&mut store, 20.);
    let c = pair(&mut store, 100.);
    let other = LoopId::from_parts(1, 4).unwrap();
    let next = LoopId::from_parts(1, 5).unwrap();
    let mut ledger = Ledger::prepare(2, 5, 2, 2).unwrap();
    ledger.bind(owner(1)).unwrap();
    ledger.bind(other).unwrap();
    assert_no_heap(|| {
        let ia = input(&mut store, a, owner(1), 0, Frame(0));
        let ma = ledger.adopt(&mut store.io(), ia).unwrap();
        let ib = input(&mut store, b, other, 0, Frame(0));
        let mb = ledger.adopt(&mut store.io(), ib).unwrap();
        ledger
            .set_playback(&mut store.io(), owner(1), Some(ma))
            .unwrap();
        ledger
            .set_latest(&mut store.io(), owner(1), Some(ma))
            .unwrap();
        ledger
            .set_playback(&mut store.io(), other, Some(mb))
            .unwrap();
        let exports = [
            ledger.export(ma, Frame(9)).unwrap(),
            ledger.export(mb, Frame(10)).unwrap(),
        ];
        let pins = [ledger.pin(ma).unwrap(), ledger.pin(mb).unwrap()];
        ledger.retire(&mut store.io(), ma).unwrap();
        ledger.retire(&mut store.io(), mb).unwrap();
        let reservation = ledger
            .reserve(Description {
                frames: 0,
                revision: Revision(7),
                ..ledger.view(ma).unwrap().description
            })
            .unwrap();
        let mut changes = [
            OwnerChange::replace(Some(owner(1)), input(&mut store, c, owner(2), 0, Frame(20))),
            OwnerChange::replace(Some(other), input(&mut store, c, next, 0, Frame(20))),
        ];
        let before = ledger.usage();
        let resources = store.usage();
        {
            let mut io = store.io();
            let _permit = ledger
                .prepare_owner_activation(&mut io, &mut changes)
                .unwrap();
        }
        assert_eq!(ledger.usage(), before);
        assert_eq!(store.usage(), resources);
        assert_eq!(ledger.playback(owner(1)), Ok(Some(ma)));
        assert_eq!(ledger.playback(other), Ok(Some(mb)));
        assert!(changes
            .iter()
            .all(|c| !c.is_applied() && c.input().is_some() && c.published().is_none()));
        ledger
            .prepare_owner_activation(&mut store.io(), &mut changes)
            .unwrap()
            .commit();
        assert!(changes
            .iter()
            .all(|c| c.is_applied() && c.input().is_none()));
        assert_eq!(ledger.usage().references, before.references + 1);
        assert_eq!(ledger.usage().free_media, 0);
        assert_eq!(store.usage(), resources);
        for (i, new) in [owner(2), next].into_iter().enumerate() {
            let media = changes[i].published().unwrap();
            assert_eq!(ledger.playback(new), Ok(Some(media)));
            assert_eq!(ledger.latest(new), Ok(None));
            let view = ledger.view(media).unwrap();
            assert_eq!(view.description.owner, new);
            verify(&mut store, view, 100.);
            assert_eq!(
                ledger.set_playback(&mut store.io(), new, Some(ma)),
                Err(Error::InvalidId)
            );
        }
        for (i, (media, old, base)) in [(ma, owner(1), 1.), (mb, other, 20.)]
            .into_iter()
            .enumerate()
        {
            assert_eq!(ledger.playback(old), Err(Error::InvalidId));
            let export = ledger.exported(exports[i]).unwrap();
            assert_eq!(
                (
                    export.media,
                    export.view.description.owner,
                    export.requested_cut
                ),
                (media, old, Frame(9 + i as u64))
            );
            verify(&mut store, export.view, base);
            assert_eq!(ledger.pinned(pins[i]), Ok(media));
            ledger.release_export(&mut store.io(), exports[i]).unwrap();
            verify(&mut store, ledger.view(media).unwrap(), base);
            ledger.release_pin(&mut store.io(), pins[i]).unwrap();
            assert_eq!(ledger.view(media), Err(Error::InvalidId));
        }
        assert_eq!(ledger.view(reservation), Err(Error::InvalidState));
        ledger.retire(&mut store.io(), reservation).unwrap();
        for change in &changes {
            ledger
                .retire(&mut store.io(), change.published().unwrap())
                .unwrap();
        }
        let mut remove = [OwnerChange::remove(owner(2)), OwnerChange::remove(next)];
        ledger
            .prepare_owner_activation(&mut store.io(), &mut remove)
            .unwrap()
            .commit();
        assert!(remove.iter().all(OwnerChange::is_applied));
        assert_eq!(
            ledger.usage(),
            Usage {
                free_media: 5,
                free_exports: 2,
                free_pins: 2,
                references: 0
            }
        );
        assert_eq!(store.usage().free_snapshots, 16);
        assert_eq!(
            ledger
                .prepare_owner_activation(&mut store.io(), &mut remove)
                .err(),
            Some(Error::InvalidState)
        );
        ledger.release_all(&mut store.io()).unwrap();
    });
    cleanup(&mut store, &[a[0], a[1], b[0], b[1], c[0], c[1]]);
}
#[shoop_test]
fn owner_batch_refusals_preserve_every_input_and_do_not_reserve_generation_or_capacity() {
    let mut store = store();
    let a = pair(&mut store, 1.);
    let b = pair(&mut store, 20.);
    let mut ledger = Ledger::prepare(2, 3, 0, 0).unwrap();
    ledger.bind(owner(1)).unwrap();
    let other = LoopId::from_parts(1, 1).unwrap();
    ledger.bind(other).unwrap();
    assert_no_heap(|| {
        let initial = input(&mut store, a, owner(1), 0, Frame(0));
        let media = ledger.adopt(&mut store.io(), initial).unwrap();
        ledger
            .set_playback(&mut store.io(), owner(1), Some(media))
            .unwrap();
        let before = ledger.usage();
        for case in 0..7 {
            let mut first = input(&mut store, b, owner(2), 0, Frame(4));
            let mut second = input(
                &mut store,
                b,
                LoopId::from_parts(1, 2).unwrap(),
                0,
                Frame(4),
            );
            let mut previous = Some(owner(1));
            let mut second_previous = Some(other);
            match case {
                0 => second.description.frames = 3,
                1 => first.description.owner = owner(1),
                2 => previous = Some(owner(99)),
                3 => {
                    second.description.owner = owner(3);
                    second_previous = Some(owner(1));
                }
                4 => first.description.owner = LoopId::from_parts(2, 2).unwrap(),
                5 => previous = None,
                6 => {
                    release_input(&mut store, second);
                    second = copy(View {
                        description: Description {
                            owner: LoopId::from_parts(1, 2).unwrap(),
                            ..first.description
                        },
                        audio: first.audio,
                        midi: first.midi,
                    });
                }
                _ => unreachable!(),
            }
            let mut changes = [
                OwnerChange::replace(previous, first),
                OwnerChange::replace(second_previous, second),
            ];
            let resource_before = store.usage();
            let expected = match case {
                0 | 4 => Error::InvalidShape,
                6 => Error::InvalidState,
                _ => Error::InvalidId,
            };
            assert_eq!(
                ledger
                    .prepare_owner_activation(&mut store.io(), &mut changes)
                    .err(),
                Some(expected)
            );
            assert_eq!(ledger.usage(), before);
            assert_eq!(store.usage(), resource_before);
            assert_eq!(ledger.playback(owner(1)), Ok(Some(media)));
            assert!(changes
                .iter()
                .all(|c| !c.is_applied() && c.input().is_some()));
            for (i, change) in changes.into_iter().enumerate() {
                let input = change.into_input().unwrap();
                if case != 6 || i == 0 {
                    release_input(&mut store, input);
                }
            }
        }
        let mut valid = [OwnerChange::replace(
            Some(owner(1)),
            input(&mut store, b, owner(2), 0, Frame(8)),
        )];
        {
            let mut io = store.io();
            let _permit = ledger
                .prepare_owner_activation(&mut io, &mut valid)
                .unwrap();
        }
        ledger
            .prepare_owner_activation(&mut store.io(), &mut valid)
            .unwrap()
            .commit();
        assert_eq!(
            ledger
                .view(valid[0].published().unwrap())
                .unwrap()
                .description
                .owner,
            owner(2)
        );
        ledger.release_all(&mut store.io()).unwrap();
    });
    let mut tight = Ledger::prepare(1, 1, 0, 0).unwrap();
    tight.bind(owner(1)).unwrap();
    assert_no_heap(|| {
        let old = input(&mut store, a, owner(1), 0, Frame(0));
        let media = tight.adopt(&mut store.io(), old).unwrap();
        tight
            .set_playback(&mut store.io(), owner(1), Some(media))
            .unwrap();
        tight.retire(&mut store.io(), media).unwrap();
        let mut changes = [OwnerChange::replace(
            Some(owner(1)),
            input(&mut store, b, owner(2), 0, Frame(4)),
        )];
        let before = tight.usage();
        assert_eq!(
            tight
                .prepare_owner_activation(&mut store.io(), &mut changes)
                .err(),
            Some(Error::Capacity)
        );
        assert_eq!(tight.usage(), before);
        verify(&mut store, tight.view(media).unwrap(), 1.);
        release_input(
            &mut store,
            changes.into_iter().next().unwrap().into_input().unwrap(),
        );
        tight.release_all(&mut store.io()).unwrap();
    });
    cleanup(&mut store, &[a[0], a[1], b[0], b[1]]);
}

#[shoop_test]
fn seeded_owner_activation_model_checks_references_generations_and_retained_media_after_every_edit()
{
    #[derive(Clone, Copy)]
    struct Held {
        media: MediaId,
        owner: LoopId,
        base: f32,
    }
    let mut store = store();
    let a = pair(&mut store, 1.);
    let b = pair(&mut store, 20.);
    let mut ledger = Ledger::prepare(2, 6, 2, 2).unwrap();
    let mut current: [Option<Held>; 2] = [None; 2];
    let mut exports: [Option<(ExportId, Held, Frame)>; 2] = [None; 2];
    let mut pins: [Option<(PinId, Held)>; 2] = [None; 2];
    let mut generations = [0u32; 2];
    let mut counts = [0usize; 8];
    let mut applied = 0;
    let mut random = 0x38d1_e45f_0bc2_9a67u64;
    assert_no_heap(|| {
        for step in 0..512 {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            let slot = (random & 1) as usize;
            let holding = ((random >> 8) & 1) as usize;
            let action = step % 8;
            counts[action] += 1;
            match action {
                0..=4 => {
                    let invalid = action == 4;
                    let generation = if invalid {
                        generations[slot].max(1)
                    } else {
                        generations[slot] + 1
                    };
                    let who = LoopId::from_parts(slot as u32, generation).unwrap();
                    let previous = if invalid && generations[slot] == 0 {
                        Some(who)
                    } else {
                        current[slot].map(|m| m.owner)
                    };
                    let (ids, base) = if random & 4 == 0 { (a, 1.) } else { (b, 20.) };
                    let data = input(&mut store, ids, who, 0, Frame(step as u64));
                    let mut changes = [OwnerChange::replace(previous, data)];
                    let capacity = ledger.usage().free_media > 0;
                    let result = ledger
                        .prepare_owner_activation(&mut store.io(), &mut changes)
                        .map(|permit| {
                            if action != 3 {
                                permit.commit();
                            }
                        });
                    if !capacity {
                        assert_eq!(result, Err(Error::Capacity));
                    } else if invalid {
                        assert_eq!(result, Err(Error::InvalidId));
                    } else {
                        assert_eq!(result, Ok(()));
                    }
                    let change = changes.into_iter().next().unwrap();
                    if capacity && !invalid && action != 3 {
                        applied += 1;
                        assert!(change.is_applied());
                        let media = change.published().unwrap();
                        assert!(change.input().is_none());
                        current[slot] = Some(Held {
                            media,
                            owner: who,
                            base,
                        });
                        generations[slot] = generation;
                        ledger.retire(&mut store.io(), media).unwrap();
                    } else {
                        assert!(!change.is_applied());
                        assert_eq!(change.published(), None);
                        release_input(&mut store, change.into_input().unwrap());
                    }
                }
                5 => {
                    let who = current[slot].map(|m| m.owner).unwrap_or_else(|| {
                        LoopId::from_parts(slot as u32, generations[slot].max(1)).unwrap()
                    });
                    let mut changes = [OwnerChange::remove(who)];
                    let result = ledger
                        .prepare_owner_activation(&mut store.io(), &mut changes)
                        .map(|p| p.commit());
                    if current[slot].is_some() {
                        assert_eq!(result, Ok(()));
                        assert!(changes[0].is_applied());
                        current[slot] = None;
                    } else {
                        assert_eq!(result, Err(Error::InvalidId));
                    }
                }
                6 => {
                    if let Some((id, _, _)) = exports[holding].take() {
                        ledger.release_export(&mut store.io(), id).unwrap();
                        assert_eq!(
                            ledger.release_export(&mut store.io(), id),
                            Err(Error::InvalidId)
                        );
                    }
                    if let Some(media) = current[slot].or(current[1 - slot]) {
                        let cut = Frame(step as u64);
                        exports[holding] =
                            Some((ledger.export(media.media, cut).unwrap(), media, cut));
                    }
                }
                7 => {
                    if let Some((id, _)) = pins[holding].take() {
                        ledger.release_pin(&mut store.io(), id).unwrap();
                        assert_eq!(
                            ledger.release_pin(&mut store.io(), id),
                            Err(Error::InvalidId)
                        );
                    }
                    if let Some(media) = current[slot].or(current[1 - slot]) {
                        pins[holding] = Some((ledger.pin(media.media).unwrap(), media));
                    }
                }
                _ => unreachable!(),
            }
            let mut unique = [None; 6];
            let mut used = 0;
            let mut references = 0;
            for held in current
                .into_iter()
                .flatten()
                .chain(exports.into_iter().flatten().map(|(_, m, _)| m))
                .chain(pins.into_iter().flatten().map(|(_, m)| m))
            {
                references += 1;
                if !unique[..used].contains(&Some(held.media)) {
                    unique[used] = Some(held.media);
                    used += 1;
                }
                let view = ledger.view(held.media).unwrap();
                assert_eq!(
                    view.description,
                    Description {
                        owner: held.owner,
                        revision: Revision(0),
                        frames: 4,
                        channels: 2,
                        midi: true
                    }
                );
                if step % 16 == 0 {
                    verify(&mut store, view, held.base);
                }
            }
            for (slot, media) in current.iter().enumerate() {
                if let Some(media) = media {
                    assert_eq!(ledger.playback(media.owner), Ok(Some(media.media)));
                    assert_eq!(ledger.latest(media.owner), Ok(None));
                } else if generations[slot] > 0 {
                    assert_eq!(
                        ledger
                            .playback(LoopId::from_parts(slot as u32, generations[slot]).unwrap()),
                        Err(Error::InvalidId)
                    );
                }
            }
            for (id, media, cut) in exports.into_iter().flatten() {
                let export = ledger.exported(id).unwrap();
                assert_eq!(
                    (
                        export.media,
                        export.view.description.owner,
                        export.requested_cut
                    ),
                    (media.media, media.owner, cut)
                );
            }
            for (id, media) in pins.into_iter().flatten() {
                assert_eq!(ledger.pinned(id), Ok(media.media));
            }
            assert_eq!(
                ledger.usage(),
                Usage {
                    free_media: 6 - used,
                    free_exports: 2 - exports.iter().flatten().count(),
                    free_pins: 2 - pins.iter().flatten().count(),
                    references
                }
            );
        }
        assert_eq!(counts, [64; 8]);
        assert!(applied > 64);
        ledger.release_all(&mut store.io()).unwrap();
        assert_eq!(ledger.usage().references, 0);
        let maximal = LoopId::from_parts(0, u32::MAX).unwrap();
        let make = || {
            OwnerChange::replace(
                None,
                Input {
                    description: Description {
                        owner: maximal,
                        revision: Revision(0),
                        frames: 0,
                        channels: 0,
                        midi: false,
                    },
                    audio: None,
                    midi: None,
                },
            )
        };
        let mut changes = [make()];
        {
            let mut io = store.io();
            let _permit = ledger
                .prepare_owner_activation(&mut io, &mut changes)
                .unwrap();
        }
        ledger
            .prepare_owner_activation(&mut store.io(), &mut changes)
            .unwrap()
            .commit();
        assert_eq!(
            ledger
                .prepare_owner_activation(&mut store.io(), &mut changes)
                .err(),
            Some(Error::InvalidState)
        );
        ledger
            .retire(&mut store.io(), changes[0].published().unwrap())
            .unwrap();
        let mut remove = [OwnerChange::remove(maximal)];
        ledger
            .prepare_owner_activation(&mut store.io(), &mut remove)
            .unwrap()
            .commit();
        let mut stale = [make()];
        assert_eq!(
            ledger
                .prepare_owner_activation(&mut store.io(), &mut stale)
                .err(),
            Some(Error::InvalidId)
        );
        assert_eq!(ledger.bind(maximal), Err(Error::InvalidId));
        ledger
            .bind(LoopId::from_parts(1, generations[1] + 1).unwrap())
            .unwrap();
        let before = ledger.usage();
        ledger
            .prepare_owner_activation(&mut store.io(), &mut [])
            .unwrap()
            .commit();
        assert_eq!(ledger.usage(), before);
        ledger.release_all(&mut store.io()).unwrap();
        assert_eq!(
            ledger.usage(),
            Usage {
                free_media: 6,
                free_exports: 2,
                free_pins: 2,
                references: 0
            }
        );
    });
    cleanup(&mut store, &[a[0], a[1], b[0], b[1]]);
}

#[shoop_test]
fn mixed_media_roles_and_saturated_exports_share_leases_and_recover_exactly_once() {
    let mut store = store();
    let a = pair(&mut store, 1.);
    let b = pair(&mut store, 20.);
    let mut ledger = Ledger::prepare(2, 4, 2, 2).unwrap();
    ledger.bind(owner(1)).unwrap();
    assert_no_heap(|| {
        let first = input(&mut store, a, owner(1), 0, Frame(0));
        let a_id = ledger.adopt(&mut store.io(), first).unwrap();
        let second = input(&mut store, b, owner(1), 1, Frame(1));
        let b_id = ledger.adopt(&mut store.io(), second).unwrap();
        ledger
            .set_playback(&mut store.io(), owner(1), Some(a_id))
            .unwrap();
        ledger
            .set_latest(&mut store.io(), owner(1), Some(a_id))
            .unwrap();
        let before = store.usage();
        let e0 = ledger.export(a_id, Frame(10)).unwrap();
        let e1 = ledger.export(a_id, Frame(11)).unwrap();
        assert_eq!(store.usage(), before);
        assert_eq!(ledger.usage().references, 6);
        assert_eq!(ledger.export(b_id, Frame(12)), Err(Error::Capacity));
        let pin = ledger.pin(a_id).unwrap();
        let second_pin = ledger.pin(b_id).unwrap();
        assert_eq!(ledger.pin(b_id), Err(Error::Capacity));
        ledger.retire(&mut store.io(), a_id).unwrap();
        assert_eq!(
            ledger.retire(&mut store.io(), a_id),
            Err(Error::InvalidState)
        );
        ledger
            .set_playback(&mut store.io(), owner(1), Some(b_id))
            .unwrap();
        ledger
            .set_latest(&mut store.io(), owner(1), Some(b_id))
            .unwrap();
        verify(&mut store, ledger.exported(e0).unwrap().view, 1.);
        let reserved = ledger
            .reserve(Description {
                frames: 0,
                revision: Revision(2),
                ..ledger.view(b_id).unwrap().description
            })
            .unwrap();
        assert_eq!(ledger.view(reserved), Err(Error::InvalidState));
        let third = input(&mut store, a, owner(1), 2, Frame(12));
        ledger.publish(&mut store.io(), reserved, third).unwrap();
        ledger
            .set_playback(&mut store.io(), owner(1), Some(reserved))
            .unwrap();
        ledger
            .set_latest(&mut store.io(), owner(1), Some(reserved))
            .unwrap();
        ledger.retire(&mut store.io(), reserved).unwrap();
        ledger.retire(&mut store.io(), b_id).unwrap();
        store.remove(a[0]).unwrap();
        verify(&mut store, ledger.exported(e1).unwrap().view, 1.);
        ledger.release_export(&mut store.io(), e0).unwrap();
        assert_eq!(
            ledger.release_export(&mut store.io(), e0),
            Err(Error::InvalidId)
        );
        verify(&mut store, ledger.exported(e1).unwrap().view, 1.);
        let replacement = ledger.export(reserved, Frame(13)).unwrap();
        assert_ne!(replacement, e0);
        assert_eq!(ledger.exported(e0), Err(Error::InvalidId));
        ledger.release_export(&mut store.io(), e1).unwrap();
        assert_eq!(ledger.pinned(pin).unwrap(), a_id);
        verify(&mut store, ledger.view(a_id).unwrap(), 1.);
        ledger.release_pin(&mut store.io(), pin).unwrap();
        ledger.release_pin(&mut store.io(), second_pin).unwrap();
        assert_eq!(ledger.view(a_id), Err(Error::InvalidId));
        ledger.unbind(&mut store.io(), owner(1)).unwrap();
        ledger.bind(owner(2)).unwrap();
        assert_eq!(
            ledger.set_playback(&mut store.io(), owner(2), Some(reserved)),
            Err(Error::InvalidId)
        );
        verify(&mut store, ledger.exported(replacement).unwrap().view, 1.);
        ledger.release_all(&mut store.io()).unwrap();
        ledger.release_all(&mut store.io()).unwrap();
        assert_eq!(
            ledger.usage(),
            Usage {
                free_media: 4,
                free_exports: 2,
                free_pins: 2,
                references: 0
            }
        );
        assert_eq!(store.usage().free_snapshots, 16);
    });
    cleanup(&mut store, &[a[1], b[0], b[1]]);
}

#[shoop_test]
fn malformed_foreign_duplicate_and_unindexed_inputs_return_owners_without_publication() {
    assert!(Ledger::prepare(0, 1, 1, 1).is_err());
    assert!(Ledger::prepare(1, 0, 1, 1).is_err());
    assert!(Ledger::prepare(usize::MAX, 1, 1, 1).is_err());
    assert!(Ledger::prepare(1, usize::MAX, 1, 1).is_err());
    assert!(Ledger::prepare(1, 1, usize::MAX, 1).is_err());
    assert!(Ledger::prepare(1, 1, 1, usize::MAX).is_err());
    assert!(MediaId::from_parts([0, 0, 1]).is_err());
    assert!(ExportId::from_parts([1, 0, 0]).is_err());
    let mut store = store();
    let ids = pair(&mut store, 2.);
    let unindexed = upload(&mut store, Format::Midi, &midi_bytes(), false);
    let mut foreign_store = crate::store();
    let foreign_ids = pair(&mut foreign_store, 40.);
    let mut ledger = Ledger::prepare(1, 2, 1, 1).unwrap();
    ledger.bind(owner(1)).unwrap();
    let mut other = Ledger::prepare(1, 2, 1, 1).unwrap();
    other.bind(owner(1)).unwrap();
    assert_no_heap(|| {
        let data = input(&mut store, ids, owner(1), 0, Frame(0));
        let original = View {
            description: data.description,
            audio: data.audio,
            midi: data.midi,
        };
        let mut wrong = copy(original);
        wrong.description.frames = 3;
        let before = ledger.usage();
        let pools = store.usage();
        let rejected = ledger.adopt(&mut store.io(), wrong).unwrap_err();
        assert_eq!(rejected.error, Error::InvalidShape);
        assert_eq!(rejected.input.audio, original.audio);
        assert_eq!(ledger.usage(), before);
        assert_eq!(store.usage(), pools);
        let mut missing = copy(original);
        missing.midi = None;
        assert_eq!(
            ledger.adopt(&mut store.io(), missing).unwrap_err().error,
            Error::InvalidShape
        );
        let mut forged = copy(original);
        forged.audio.as_mut().unwrap().snapshot.cut_frame = Frame(999);
        assert_eq!(
            ledger.adopt(&mut store.io(), forged).unwrap_err().error,
            Error::InvalidId
        );
        let foreign = input(&mut foreign_store, foreign_ids, owner(1), 0, Frame(0));
        let rejected = ledger.adopt(&mut store.io(), foreign).unwrap_err();
        assert_eq!(rejected.error, Error::InvalidId);
        release_input(&mut foreign_store, rejected.input);
        let snapshot = store.snapshot(unindexed, Frame(0)).unwrap();
        let mut bad = copy(original);
        bad.midi = Some(Source {
            snapshot,
            index: store.read_index(snapshot.id).unwrap(),
        });
        assert_eq!(
            ledger.adopt(&mut store.io(), bad).unwrap_err().error,
            Error::InvalidState
        );
        store.release_snapshot(snapshot.id).unwrap();
        let id = ledger.adopt(&mut store.io(), data).unwrap();
        assert_eq!(
            ledger
                .adopt(&mut store.io(), copy(original))
                .unwrap_err()
                .error,
            Error::InvalidState
        );
        assert_eq!(other.view(id), Err(Error::InvalidId));
        let exp = ledger.export(id, Frame(2)).unwrap();
        assert_eq!(other.exported(exp), Err(Error::InvalidId));
        let pin = ledger.pin(id).unwrap();
        assert_eq!(other.pinned(pin), Err(Error::InvalidId));
        assert_eq!(
            other.release_pin(&mut store.io(), pin),
            Err(Error::InvalidId)
        );
        assert_eq!(
            other.release_export(&mut store.io(), exp),
            Err(Error::InvalidId)
        );
        let fresh = input(&mut foreign_store, foreign_ids, owner(1), 1, Frame(0));
        let rejected = ledger.adopt(&mut foreign_store.io(), fresh).unwrap_err();
        assert_eq!(rejected.error, Error::InvalidId);
        release_input(&mut foreign_store, rejected.input);
        let reserved = ledger
            .reserve(Description {
                frames: 0,
                revision: Revision(2),
                ..original.description
            })
            .unwrap();
        assert_eq!(ledger.export(reserved, Frame(3)), Err(Error::InvalidState));
        assert_eq!(
            ledger.set_playback(&mut store.io(), owner(1), Some(reserved)),
            Err(Error::InvalidState)
        );
        let fresh = input(&mut store, ids, owner(1), 3, Frame(3));
        let rejected = ledger
            .publish(&mut store.io(), reserved, fresh)
            .unwrap_err();
        assert_eq!(rejected.error, Error::InvalidShape);
        let mut fixed = rejected.input;
        fixed.description.revision = Revision(2);
        ledger.publish(&mut store.io(), reserved, fixed).unwrap();
        let nonexistent = MediaId::from_parts([id.parts()[0], u32::MAX, 1]).unwrap();
        assert_eq!(
            ledger.retire(&mut store.io(), nonexistent),
            Err(Error::InvalidId)
        );
        ledger.release_all(&mut store.io()).unwrap();
        assert_eq!(ledger.usage().references, 0);
        assert_eq!(store.usage().free_snapshots, 16);
    });
    cleanup(&mut store, &[ids[0], ids[1], unindexed]);
    cleanup(&mut foreign_store, &foreign_ids);
}

#[shoop_test]
fn optional_metadata_capacity_and_cancelled_reservations_do_not_steal_internal_roles() {
    let mut store = store();
    let ids = pair(&mut store, 3.);
    let mut ledger = Ledger::prepare(1, 1, 0, 0).unwrap();
    ledger.bind(owner(1)).unwrap();
    assert_no_heap(|| {
        let description = Description {
            owner: owner(1),
            revision: Revision(1),
            frames: 0,
            channels: 2,
            midi: true,
        };
        let first = ledger.reserve(description).unwrap();
        assert_eq!(ledger.reserve(description), Err(Error::Capacity));
        ledger.retire(&mut store.io(), first).unwrap();
        let second = ledger.reserve(description).unwrap();
        assert_ne!(first, second);
        assert_eq!(ledger.retire(&mut store.io(), first), Err(Error::InvalidId));
        let data = input(&mut store, ids, owner(1), 1, Frame(0));
        ledger.publish(&mut store.io(), second, data).unwrap();
        ledger
            .set_playback(&mut store.io(), owner(1), Some(second))
            .unwrap();
        ledger
            .set_latest(&mut store.io(), owner(1), Some(second))
            .unwrap();
        ledger.retire(&mut store.io(), second).unwrap();
        assert_eq!(ledger.export(second, Frame(0)), Err(Error::Capacity));
        for _ in 0..128 {
            ledger
                .set_playback(&mut store.io(), owner(1), Some(second))
                .unwrap();
            ledger
                .set_latest(&mut store.io(), owner(1), Some(second))
                .unwrap();
            assert_eq!(ledger.usage().references, 2);
        }
        assert_eq!(ledger.playback(owner(1)).unwrap(), Some(second));
        assert_eq!(ledger.latest(owner(1)).unwrap(), Some(second));
        assert_eq!(ledger.bind(owner(1)), Err(Error::InvalidId));
        ledger.unbind(&mut store.io(), owner(1)).unwrap();
        assert_eq!(ledger.bind(owner(1)), Err(Error::InvalidId));
        ledger.bind(owner(u32::MAX)).unwrap();
        ledger.unbind(&mut store.io(), owner(u32::MAX)).unwrap();
        assert_eq!(ledger.bind(owner(u32::MAX)), Err(Error::InvalidId));
        assert_eq!(ledger.usage().free_media, 1);
    });
    cleanup(&mut store, &ids);
}

#[derive(Clone, Copy)]
struct Model {
    key: MediaId,
    owner: LoopId,
    resident: bool,
}
#[shoop_test]
fn seeded_ownership_model_preserves_every_live_export_and_all_fixed_pool_credits() {
    let mut store = store();
    let ids = pair(&mut store, 5.);
    let mut ledger = Ledger::prepare(1, 4, 2, 3).unwrap();
    let mut current = owner(1);
    ledger.bind(current).unwrap();
    let mut model: [Option<Model>; 4] = [None; 4];
    let mut exports: [Option<(ExportId, MediaId)>; 3] = [None; 3];
    let mut pins: [Option<(PinId, MediaId)>; 2] = [None; 2];
    let mut playback = None;
    let mut latest = None;
    let mut random = 0x87a4439cdd3517abu64;
    let mut actions = [0; 10];
    for step in 0..2048 {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        let action = ((random >> 16) % 10) as usize;
        let selected = model[((random >> 32) as usize) % 4];
        let export_slot = ((random >> 40) as usize) % 3;
        assert_no_heap(|| {
            match action {
                0 if model.iter().any(Option::is_none) => {
                    let data = input(&mut store, ids, current, step, Frame(step));
                    let key = ledger.adopt(&mut store.io(), data).unwrap();
                    let slot = key.parts()[1] as usize;
                    assert!(model[slot].is_none());
                    model[slot] = Some(Model {
                        key,
                        owner: current,
                        resident: true,
                    });
                    actions[0] += 1;
                }
                1 => {
                    if let Some(row) = selected {
                        let result = ledger.retire(&mut store.io(), row.key);
                        if row.resident {
                            result.unwrap();
                            model[row.key.parts()[1] as usize]
                                .as_mut()
                                .unwrap()
                                .resident = false;
                            actions[1] += 1;
                        } else {
                            assert_eq!(result, Err(Error::InvalidState));
                        }
                    }
                }
                2 | 3 => {
                    let key = selected.filter(|m| m.owner == current).map(|m| m.key);
                    if action == 2 {
                        ledger.set_playback(&mut store.io(), current, key).unwrap();
                        playback = key;
                    } else {
                        ledger.set_latest(&mut store.io(), current, key).unwrap();
                        latest = key;
                    }
                    actions[action] += 1;
                }
                4 => {
                    if let Some(row) = selected {
                        let result = ledger.export(row.key, Frame(step));
                        if exports.iter().any(Option::is_none) {
                            let key = result.unwrap();
                            let slot = key.parts()[1] as usize;
                            assert!(exports[slot].is_none());
                            exports[slot] = Some((key, row.key));
                            actions[4] += 1;
                        } else {
                            assert_eq!(result, Err(Error::Capacity));
                        }
                    }
                }
                5 => {
                    if let Some((key, _)) = exports[export_slot] {
                        ledger.release_export(&mut store.io(), key).unwrap();
                        exports[export_slot] = None;
                        assert_eq!(
                            ledger.release_export(&mut store.io(), key),
                            Err(Error::InvalidId)
                        );
                        actions[5] += 1;
                    }
                }
                6 => {
                    if let Some(row) = selected {
                        verify(&mut store, ledger.view(row.key).unwrap(), 5.);
                        actions[6] += 1;
                    }
                }
                7 => {
                    ledger.unbind(&mut store.io(), current).unwrap();
                    playback = None;
                    latest = None;
                    current = owner(current.generation() + 1);
                    ledger.bind(current).unwrap();
                    actions[7] += 1;
                }
                8 => {
                    if let Some(row) = selected {
                        let result = ledger.pin(row.key);
                        if pins.iter().any(Option::is_none) {
                            let key = result.unwrap();
                            let slot = key.parts()[1] as usize;
                            assert!(pins[slot].is_none());
                            pins[slot] = Some((key, row.key));
                            actions[8] += 1;
                        } else {
                            assert_eq!(result, Err(Error::Capacity));
                        }
                    }
                }
                9 => {
                    let slot = ((random >> 48) as usize) % 2;
                    if let Some((key, _)) = pins[slot] {
                        ledger.release_pin(&mut store.io(), key).unwrap();
                        pins[slot] = None;
                        assert_eq!(
                            ledger.release_pin(&mut store.io(), key),
                            Err(Error::InvalidId)
                        );
                        actions[9] += 1;
                    }
                }
                _ => {}
            }
            let mut references = 0;
            let mut live = 0;
            for row in &mut model {
                if let Some(value) = *row {
                    let refs = usize::from(value.resident)
                        + usize::from(playback == Some(value.key))
                        + usize::from(latest == Some(value.key))
                        + exports
                            .iter()
                            .flatten()
                            .filter(|(_, key)| *key == value.key)
                            .count()
                        + pins
                            .iter()
                            .flatten()
                            .filter(|(_, key)| *key == value.key)
                            .count();
                    if refs == 0 {
                        assert_eq!(ledger.view(value.key), Err(Error::InvalidId));
                        *row = None;
                    } else {
                        references += refs;
                        live += 1;
                        assert_eq!(
                            ledger.view(value.key).unwrap().description.owner,
                            value.owner
                        );
                    }
                }
            }
            assert_eq!(
                ledger.usage(),
                Usage {
                    free_media: 4 - live,
                    free_exports: exports.iter().filter(|e| e.is_none()).count(),
                    free_pins: pins.iter().filter(|e| e.is_none()).count(),
                    references
                }
            );
            assert_eq!(store.usage().free_snapshots, 16 - 2 * live);
            assert_eq!(ledger.playback(current).unwrap(), playback);
            assert_eq!(ledger.latest(current).unwrap(), latest);
            for (pin, key) in pins.iter().flatten() {
                assert_eq!(ledger.pinned(*pin).unwrap(), *key);
            }
            for (export, key) in exports.iter().flatten() {
                let view = ledger.exported(*export).unwrap();
                assert_eq!(view.media, *key);
                if step % 32 == 0 {
                    verify(&mut store, view.view, 5.);
                }
            }
        });
    }
    assert!(actions.iter().all(|n| *n > 20), "{actions:?}");
    assert_no_heap(|| {
        ledger.release_all(&mut store.io()).unwrap();
        assert_eq!(
            ledger.usage(),
            Usage {
                free_media: 4,
                free_exports: 3,
                free_pins: 2,
                references: 0
            }
        );
    });
    cleanup(&mut store, &ids);
}
