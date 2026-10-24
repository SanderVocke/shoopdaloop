mod support;

use rt_runtime::resource::*;
use rt_runtime::{Budgets, Config, Error, Frame, ResourceId, Revision};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn store() -> ResourceStore {
    ResourceStore::new(
        Config {
            resource_slots: 8,
            resource_pages: 64,
            page_bytes: 16,
            midi_bytes_per_port: 64,
            budgets: Budgets {
                transfer_bytes: 16,
                validation_steps: 16,
                ..Budgets::default()
            },
            ..Config::default()
        }
        .prepare()
        .unwrap(),
    )
    .unwrap()
}
fn upload(
    store: &mut ResourceStore,
    resource: ResourceId,
    revision: Revision,
    description: Description,
    data: &[u8],
) -> UploadId {
    let id = assert_no_heap(|| store.begin_upload(resource, revision, description)).unwrap();
    for (index, chunk) in data.chunks(16).enumerate() {
        assert_no_heap(|| {
            store.begin_callback();
            store.write_upload(id, index * 16, chunk)
        })
        .unwrap();
        assert!(store.work().copied_bytes <= 16);
        assert!(store.work().validation_steps <= 16);
    }
    assert_no_heap(|| store.finish_upload(id)).unwrap();
    id
}
fn fetch(store: &mut ResourceStore, snapshot: Snapshot) -> Vec<u8> {
    let mut result = Vec::with_capacity(snapshot.description.bytes);
    let mut cursor = assert_no_heap(|| store.cursor(snapshot.id)).unwrap();
    let mut chunk = [0; 16];
    loop {
        let count = assert_no_heap(|| {
            store.begin_callback();
            store.read(&mut cursor, &mut chunk)
        })
        .unwrap();
        assert!(store.work().copied_bytes <= 16);
        if count == 0 {
            break;
        }
        result.extend_from_slice(&chunk[..count]);
    }
    assert_eq!(cursor.position(), snapshot.description.bytes);
    result
}
fn reclaim_all(store: &mut ResourceStore) {
    for _ in 0..128 {
        let steps = assert_no_heap(|| {
            store.begin_callback();
            store.reclaim()
        });
        assert!(steps <= 16);
        if store.usage().retired_pages == 0 && store.usage().retired_versions == 0 {
            return;
        }
    }
    panic!("reclamation exceeded fixed pool bound");
}

#[shoop_test]
fn large_audio_revisions_remain_immutable_during_replacement_and_incremental_reclamation() {
    let mut store = store();
    let resource = assert_no_heap(|| store.create()).unwrap();
    let original: Vec<u8> = (0..128)
        .flat_map(|sample| (sample as f32 * 0.125).to_le_bytes())
        .collect();
    let replacement: Vec<u8> = (0..128)
        .flat_map(|sample| (-(sample as f32)).to_le_bytes())
        .collect();
    let description = Description {
        format: Format::Audio { channels: 2 },
        frames: 64,
        bytes: original.len(),
    };
    let first = upload(&mut store, resource, Revision(0), description, &original);
    assert_eq!(assert_no_heap(|| store.commit(first)), Ok(Revision(1)));
    let snapshot = assert_no_heap(|| store.snapshot(resource, Frame(100))).unwrap();
    assert_eq!(snapshot.cut_frame, Frame(100));
    assert_eq!(store.currentness(snapshot.id), Ok(Currentness::Current));
    let next = upload(&mut store, resource, Revision(1), description, &replacement);
    assert_eq!(store.usage().free_pages, 0);
    assert_eq!(assert_no_heap(|| store.commit(next)), Ok(Revision(2)));
    assert_eq!(store.currentness(snapshot.id), Ok(Currentness::Stale));
    assert_eq!(fetch(&mut store, snapshot), original);
    let newest = assert_no_heap(|| store.snapshot(resource, Frame(200))).unwrap();
    assert_eq!(fetch(&mut store, newest), replacement);
    assert_eq!(store.usage().retired_versions, 0); // Old revision is pinned.
    assert_no_heap(|| store.release_snapshot(snapshot.id)).unwrap();
    assert_eq!(store.usage().retired_versions, 1);
    let before = store.usage().free_pages;
    assert_no_heap(|| {
        store.begin_callback();
        assert_eq!(store.reclaim(), 16);
    });
    assert_eq!(store.usage().free_pages, before + 16);
    assert_eq!(store.usage().retired_versions, 1);
    reclaim_all(&mut store);
    assert_eq!(store.usage().free_pages, 32);
    assert_no_heap(|| store.remove(resource)).unwrap();
    assert_eq!(
        store.currentness(newest.id),
        Ok(Currentness::ResourceRemoved)
    );
    assert_eq!(fetch(&mut store, newest), replacement);
    assert_no_heap(|| store.release_snapshot(newest.id)).unwrap();
    reclaim_all(&mut store);
    assert_eq!(
        store.usage(),
        PoolUsage {
            free_pages: 64,
            reserved_pages: 0,
            free_versions: 8,
            free_snapshots: 8,
            retired_pages: 0,
            retired_versions: 0
        }
    );
}

#[shoop_test]
fn chunk_order_completeness_and_callback_copy_budget_are_enforced() {
    let mut store = store();
    let resource = store.create().unwrap();
    let id = store
        .begin_upload(
            resource,
            Revision(0),
            Description {
                format: Format::Audio { channels: 1 },
                frames: 8,
                bytes: 32,
            },
        )
        .unwrap();
    assert_eq!(store.usage().reserved_pages, 2);
    assert_no_heap(|| {
        store.begin_callback();
        assert_eq!(store.finish_upload(id), Err(Error::Incomplete));
        assert_eq!(store.commit(id), Err(Error::Incomplete));
        assert_eq!(
            store.write_upload(id, 16, &[1; 16]),
            Err(Error::InvalidOffset)
        );
        assert_eq!(
            store.write_upload(id, 0, &[1; 17]),
            Err(Error::InvalidShape)
        );
        store.write_upload(id, 0, &[1; 16]).unwrap();
        assert_eq!(
            store.write_upload(id, 0, &[1; 16]),
            Err(Error::InvalidOffset)
        );
        assert_eq!(store.write_upload(id, 16, &[2; 16]), Err(Error::Budget));
        assert_eq!(store.work().copied_bytes, 16);
        store.begin_callback();
        store.write_upload(id, 16, &[2; 16]).unwrap();
        store.finish_upload(id).unwrap();
        assert_eq!(store.write_upload(id, 32, &[1]), Err(Error::InvalidState));
        store.commit(id).unwrap();
    });
    let snapshot = store.snapshot(resource, Frame(0)).unwrap();
    let mut expected = vec![1; 16];
    expected.extend_from_slice(&[2; 16]);
    assert_eq!(fetch(&mut store, snapshot), expected);
}

#[shoop_test]
fn streamed_midi_records_round_trip_across_header_payload_and_page_boundaries() {
    let mut store = store();
    let resource = store.create().unwrap();
    let mut data = vec![];
    for (frame, packet) in [
        (2, &[0x90, 60, 100][..]),
        (9, &[0xf0, 0x7d, 1, 2, 0xf7][..]),
    ] {
        let mut buffer = [0; 64];
        let count = assert_no_heap(|| encode_midi_record(frame, packet, &mut buffer)).unwrap();
        data.extend_from_slice(&buffer[..count]);
    }
    let description = Description {
        format: Format::Midi,
        frames: 100,
        bytes: data.len(),
    };
    let transfer = store
        .begin_upload(resource, Revision(0), description)
        .unwrap();
    // Single-byte chunks exercise every parser transition independently.
    for (offset, byte) in data.iter().enumerate() {
        assert_no_heap(|| {
            store.begin_callback();
            store.write_upload(transfer, offset, &[*byte])
        })
        .unwrap();
    }
    assert_no_heap(|| {
        store.finish_upload(transfer).unwrap();
        store.commit(transfer).unwrap();
    });
    let snapshot = store.snapshot(resource, Frame(500)).unwrap();
    assert_eq!(snapshot.description.frames, 100); // Not the final event time.
    assert_eq!(fetch(&mut store, snapshot), data);
}

#[shoop_test]
fn malformed_midi_chunks_roll_back_parser_and_bytes_but_consume_validation_work() {
    let mut store = store();
    let resource = store.create().unwrap();
    let mut packet = [0; 15];
    encode_midi_record(0, &[0x90, 60, 100], &mut packet).unwrap();
    let mut malformed = packet;
    malformed[13] = 0xff;
    let transfer = store
        .begin_upload(
            resource,
            Revision(0),
            Description {
                format: Format::Midi,
                frames: 1,
                bytes: 15,
            },
        )
        .unwrap();
    assert_no_heap(|| {
        store.begin_callback();
        assert_eq!(
            store.write_upload(transfer, 0, &malformed),
            Err(Error::InvalidEvent)
        );
        assert_eq!(store.work().copied_bytes, 0);
        assert_eq!(store.work().validation_steps, 15);
        assert_eq!(store.write_upload(transfer, 0, &packet), Err(Error::Budget));
        store.begin_callback();
        store.write_upload(transfer, 0, &packet).unwrap();
        store.finish_upload(transfer).unwrap();
        store.commit(transfer).unwrap();
    });
    let snapshot = store.snapshot(resource, Frame(0)).unwrap();
    assert_eq!(fetch(&mut store, snapshot), packet);
}

#[shoop_test]
fn incomplete_and_out_of_order_midi_cannot_become_active() {
    let mut store = store();
    let resource = store.create().unwrap();
    let transfer = store
        .begin_upload(
            resource,
            Revision(0),
            Description {
                format: Format::Midi,
                frames: 1,
                bytes: 4,
            },
        )
        .unwrap();
    assert_no_heap(|| {
        store.begin_callback();
        store.write_upload(transfer, 0, &[0; 4]).unwrap();
        assert_eq!(store.finish_upload(transfer), Err(Error::Incomplete));
        store.cancel_upload(transfer).unwrap();
    });
    reclaim_all(&mut store);
    let mut packet = [0; 26];
    encode_midi_record(2, &[0xf8], &mut packet[..13]).unwrap();
    encode_midi_record(1, &[0xf8], &mut packet[13..]).unwrap();
    let transfer = store
        .begin_upload(
            resource,
            Revision(0),
            Description {
                format: Format::Midi,
                frames: 3,
                bytes: 26,
            },
        )
        .unwrap();
    assert_no_heap(|| {
        store.begin_callback();
        store.write_upload(transfer, 0, &packet[..16]).unwrap();
        store.begin_callback();
        assert_eq!(
            store.write_upload(transfer, 16, &packet[16..]),
            Err(Error::InvalidFrame)
        );
        assert_eq!(store.revision(resource), Ok(Revision(0)));
        store.cancel_upload(transfer).unwrap();
    });
    reclaim_all(&mut store);
    assert_eq!(store.usage().free_pages, 64);
}

#[shoop_test]
fn cancellation_returns_unused_reservations_and_stale_handles_cannot_touch_reused_slots() {
    let mut store = store();
    let resource = store.create().unwrap();
    let description = Description {
        format: Format::Audio { channels: 1 },
        frames: 16,
        bytes: 64,
    };
    let transfer = store
        .begin_upload(resource, Revision(0), description)
        .unwrap();
    assert_no_heap(|| {
        store.begin_callback();
        store.write_upload(transfer, 0, &[1; 16]).unwrap();
        store.cancel_upload(transfer).unwrap();
        assert_eq!(store.cancel_upload(transfer), Err(Error::InvalidState));
    });
    assert_eq!(store.usage().reserved_pages, 0);
    assert_eq!(store.usage().free_pages, 63);
    reclaim_all(&mut store);
    let next = store
        .begin_upload(resource, Revision(0), description)
        .unwrap();
    assert_eq!(next.slot, transfer.slot);
    assert_ne!(next.generation, transfer.generation);
    assert_no_heap(|| {
        assert_eq!(
            store.write_upload(transfer, 0, &[2; 16]),
            Err(Error::InvalidId)
        );
        store.cancel_upload(next).unwrap();
    });
    reclaim_all(&mut store);
    assert_eq!(store.usage().free_versions, 8);
    assert_eq!(store.usage().free_pages, 64);
}

#[shoop_test]
fn pinned_snapshots_and_reservations_exhaust_without_overcommitting_or_invalidating_readers() {
    let mut store = store();
    let resource = store.create().unwrap();
    let empty = Description {
        format: Format::Audio { channels: 1 },
        frames: 0,
        bytes: 0,
    };
    let first = upload(&mut store, resource, Revision(0), empty, &[]);
    let stale = upload(&mut store, resource, Revision(0), empty, &[]);
    store.commit(first).unwrap();
    assert_eq!(store.commit(stale), Err(Error::StaleRevision));
    store.cancel_upload(stale).unwrap();
    reclaim_all(&mut store);
    let snapshots: Vec<_> = (0..8)
        .map(|_| store.snapshot(resource, Frame(0)).unwrap())
        .collect();
    assert_eq!(store.snapshot(resource, Frame(0)), Err(Error::Capacity));
    let mut cursor = store.cursor(snapshots[0].id).unwrap();
    store.remove(resource).unwrap();
    let replacement = store.create().unwrap();
    assert_eq!(replacement.slot(), resource.slot());
    assert_ne!(replacement.generation(), resource.generation());
    assert_eq!(
        store.currentness(snapshots[0].id),
        Ok(Currentness::ResourceRemoved)
    );
    for snapshot in snapshots {
        assert_no_heap(|| store.release_snapshot(snapshot.id)).unwrap();
    }
    assert_eq!(store.read(&mut cursor, &mut [0; 1]), Err(Error::InvalidId));
    reclaim_all(&mut store);
    let huge = Description {
        format: Format::Audio { channels: 1 },
        frames: 256,
        bytes: 1024,
    };
    let reserved = store.begin_upload(replacement, Revision(0), huge).unwrap();
    assert_eq!(store.usage().reserved_pages, 64);
    assert_eq!(
        store.begin_upload(
            replacement,
            Revision(0),
            Description {
                format: Format::Audio { channels: 1 },
                frames: 1,
                bytes: 4
            }
        ),
        Err(Error::Capacity)
    );
    store.cancel_upload(reserved).unwrap();
    reclaim_all(&mut store);
    assert_eq!(store.usage().free_versions, 8);
}

#[shoop_test]
fn invalid_resource_metadata_does_not_reserve_pages_or_descriptors() {
    let mut store = store();
    let resource = store.create().unwrap();
    let before = store.usage();
    assert_no_heap(|| {
        for description in [
            Description {
                format: Format::Audio { channels: 0 },
                frames: 0,
                bytes: 0,
            },
            Description {
                format: Format::Audio { channels: 2 },
                frames: u64::MAX,
                bytes: 0,
            },
            Description {
                format: Format::Audio { channels: 1 },
                frames: 2,
                bytes: 7,
            },
            Description {
                format: Format::Midi,
                frames: 0,
                bytes: 1,
            },
            Description {
                format: Format::Midi,
                frames: 1,
                bytes: usize::MAX,
            },
        ] {
            assert!(store
                .begin_upload(resource, Revision(0), description)
                .is_err());
        }
        assert_eq!(store.usage(), before);
    });
}

#[shoop_test]
fn streaming_short_message_parser_matches_complete_packet_validation() {
    let mut store = store();
    for status in 0u8..=255 {
        for len in 1..=4 {
            let resource = store.create().unwrap();
            let mut record = [0; 16];
            record[8..12].copy_from_slice(&(len as u32).to_le_bytes());
            record[12..].copy_from_slice(&[status, 1, 2, 3]);
            let expected = rt_runtime::midi::Message::parse(&record[12..12 + len], 64).is_ok();
            let transfer = store
                .begin_upload(
                    resource,
                    Revision(0),
                    Description {
                        format: Format::Midi,
                        frames: 1,
                        bytes: 12 + len,
                    },
                )
                .unwrap();
            let accepted = assert_no_heap(|| {
                store.begin_callback();
                store
                    .write_upload(transfer, 0, &record[..12 + len])
                    .and_then(|()| store.finish_upload(transfer))
                    .is_ok()
            });
            assert_eq!(accepted, expected, "status {status:02x} length {len}");
            store.cancel_upload(transfer).unwrap();
            store.remove(resource).unwrap();
            reclaim_all(&mut store);
        }
    }
}
