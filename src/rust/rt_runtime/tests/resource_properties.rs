mod support;
use rt_runtime::resource::*;
use rt_runtime::{Budgets, Config, Error, Frame, ResourceId, Revision};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

struct Random(u32);
impl Random {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
}
#[derive(Clone, Copy)]
struct Content {
    resource: ResourceId,
    revision: Revision,
    description: Description,
    bytes: [u8; 64],
}
#[derive(Clone, Copy)]
struct Transfer {
    id: UploadId,
    content: Content,
    written: usize,
    complete: bool,
}
#[derive(Clone, Copy)]
struct Reader {
    snapshot: Snapshot,
    content: Content,
    cursor: Cursor,
    position: usize,
}
#[derive(Debug, Default)]
struct Coverage {
    commits: usize,
    reads: usize,
    refusals: usize,
    malformed: usize,
    removals: usize,
    peak_versions: usize,
    peak_leases: usize,
}

#[shoop_test]
fn seeded_upload_snapshot_chunk_model_preserves_prefixes_and_recovers_all_pool_owners() {
    let mut coverage = Coverage::default();
    let seeds = if cfg!(miri) { 2 } else { 8 };
    let iterations = if cfg!(miri) { 128 } else { 8192 };
    for seed in 1..=seeds {
        let config = Config {
            resource_slots: 8,
            resource_pages: 32,
            page_bytes: 16,
            budgets: Budgets {
                transfer_bytes: 16,
                validation_steps: 16,
                ..Budgets::default()
            },
            ..Config::default()
        }
        .prepare()
        .unwrap();
        let mut store = ResourceStore::new(config).unwrap();
        let mut resource = assert_no_heap(|| store.create()).unwrap();
        let mut revision = Revision(0);
        let mut active: Option<Content> = None;
        let mut transfers: [Option<Transfer>; 4] = [None; 4];
        let mut readers: [Option<Reader>; 8] = [None; 8];
        let mut stale: [Option<UploadId>; 16] = [None; 16];
        let mut stale_at = 0;
        let mut random = Random(0xc46a_5f17 ^ seed);
        assert_no_heap(|| {
            for iteration in 0..iterations {
                store.begin_callback();
                let index = random.next() as usize % 4;
                let lease = random.next() as usize % 8;
                match random.next() % 16 {
                    0..=2 if transfers[index].is_none() => {
                        let midi = random.next() & 1 == 0;
                        let length = if midi {
                            random.next() as usize % 5 * 15
                        } else {
                            random.next() as usize % 17 * 4
                        };
                        let description = Description {
                            format: if midi {
                                Format::Midi
                            } else {
                                Format::Audio { channels: 1 }
                            },
                            frames: if midi { 100 } else { length as u64 / 4 },
                            bytes: length,
                        };
                        let expected = if random.next() % 8 == 0 {
                            Revision(revision.0 + 1)
                        } else {
                            revision
                        };
                        let usage = store.usage();
                        let result = store.begin_upload(resource, expected, description);
                        let model = if expected != revision {
                            Err(Error::StaleRevision)
                        } else if usage.free_versions == 0
                            || length.div_ceil(16) > usage.free_pages - usage.reserved_pages
                        {
                            Err(Error::Capacity)
                        } else {
                            Ok(())
                        };
                        assert_eq!(result.map(|_| ()), model);
                        if let Ok(id) = result {
                            let mut bytes = [0u8; 64];
                            if midi {
                                for record in 0..length / 15 {
                                    let start = record * 15;
                                    bytes[start..start + 8]
                                        .copy_from_slice(&(record as u64 * 3).to_le_bytes());
                                    bytes[start + 8..start + 12]
                                        .copy_from_slice(&3u32.to_le_bytes());
                                    bytes[start + 12..start + 15].copy_from_slice(&[0x90, 60, 100]);
                                }
                            } else {
                                for byte in &mut bytes[..length] {
                                    *byte = random.next() as u8;
                                }
                            }
                            transfers[index] = Some(Transfer {
                                id,
                                content: Content {
                                    resource,
                                    revision: expected,
                                    description,
                                    bytes,
                                },
                                written: 0,
                                complete: false,
                            });
                        } else {
                            coverage.refusals += 1;
                        }
                    }
                    3..=6 if transfers[index].is_some() => {
                        let mut transfer = transfers[index].unwrap();
                        for _ in 0..2 {
                            let offset = if random.next() % 4 == 0 {
                                random.next() as usize % 65
                            } else {
                                transfer.written
                            };
                            let remaining = transfer.content.description.bytes - transfer.written;
                            let length = if random.next() % 4 == 0 {
                                random.next() as usize % 21
                            } else {
                                remaining.min(16 - transfer.written % 16)
                            };
                            let mut bytes = [0; 20];
                            for (at, byte) in bytes[..length].iter_mut().enumerate() {
                                *byte = transfer
                                    .content
                                    .bytes
                                    .get(offset + at)
                                    .copied()
                                    .unwrap_or(0);
                            }
                            let midi = transfer.content.description.format == Format::Midi;
                            let mut malformed = false;
                            if midi && random.next() % 4 == 0 {
                                for (at, byte) in bytes[..length].iter_mut().enumerate() {
                                    if (offset + at) % 15 == 13 {
                                        *byte = 0xff;
                                        malformed = true;
                                        break;
                                    }
                                }
                            }
                            let work = store.work();
                            let usage = store.usage();
                            let expected = if transfer.complete {
                                Err(Error::InvalidState)
                            } else if offset != transfer.written {
                                Err(Error::InvalidOffset)
                            } else if length == 0 || length > remaining || length > 16 - offset % 16
                            {
                                Err(Error::InvalidShape)
                            } else if length > 16 - work.copied_bytes
                                || (midi && length > 16 - work.validation_steps)
                            {
                                Err(Error::Budget)
                            } else if malformed {
                                Err(Error::InvalidEvent)
                            } else {
                                Ok(())
                            };
                            assert_eq!(
                                store.write_upload(transfer.id, offset, &bytes[..length]),
                                expected
                            );
                            if expected.is_ok() {
                                transfer.written += length;
                            } else {
                                assert_eq!(store.usage(), usage);
                                coverage.refusals += 1;
                                if expected == Err(Error::InvalidEvent) {
                                    coverage.malformed += 1;
                                    assert_eq!(
                                        store.work().validation_steps,
                                        work.validation_steps + length
                                    );
                                }
                            }
                        }
                        transfers[index] = Some(transfer);
                    }
                    7 if transfers[index].is_some() => {
                        let transfer = transfers[index].as_mut().unwrap();
                        let expected = if transfer.complete {
                            Err(Error::InvalidState)
                        } else if transfer.written != transfer.content.description.bytes {
                            Err(Error::Incomplete)
                        } else {
                            Ok(())
                        };
                        assert_eq!(store.finish_upload(transfer.id), expected);
                        if expected.is_ok() {
                            transfer.complete = true;
                        } else {
                            coverage.refusals += 1;
                        }
                    }
                    8 if transfers[index].is_some() => {
                        let transfer = transfers[index].unwrap();
                        let expected = if !transfer.complete {
                            Err(Error::Incomplete)
                        } else if transfer.content.resource != resource {
                            Err(Error::InvalidId)
                        } else if transfer.content.revision != revision {
                            Err(Error::StaleRevision)
                        } else {
                            Ok(Revision(revision.0 + 1))
                        };
                        assert_eq!(store.commit(transfer.id), expected);
                        if let Ok(next) = expected {
                            revision = next;
                            active = Some(Content {
                                revision,
                                ..transfer.content
                            });
                            transfers[index] = None;
                            stale[stale_at % 16] = Some(transfer.id);
                            stale_at += 1;
                            coverage.commits += 1;
                        } else {
                            coverage.refusals += 1;
                        }
                    }
                    9 if transfers[index].is_some() => {
                        let transfer = transfers[index].take().unwrap();
                        store.cancel_upload(transfer.id).unwrap();
                        stale[stale_at % 16] = Some(transfer.id);
                        stale_at += 1;
                    }
                    10 if readers[lease].is_none() => {
                        let free = store.usage().free_snapshots;
                        let result = store.snapshot(resource, Frame(iteration as u64));
                        let expected = if active.is_none() {
                            Err(Error::InvalidState)
                        } else if free == 0 {
                            Err(Error::Capacity)
                        } else {
                            Ok(())
                        };
                        assert_eq!(result.map(|_| ()), expected);
                        if let Ok(snapshot) = result {
                            let content = active.unwrap();
                            assert_eq!(snapshot.description, content.description);
                            assert_eq!(snapshot.revision, revision);
                            readers[lease] = Some(Reader {
                                snapshot,
                                content,
                                cursor: store.cursor(snapshot.id).unwrap(),
                                position: 0,
                            });
                        }
                    }
                    11..=12 if readers[lease].is_some() => {
                        let reader = readers[lease].as_mut().unwrap();
                        if random.next() % 4 == 0 {
                            reader.cursor = store.cursor(reader.snapshot.id).unwrap();
                            reader.position = 0;
                        }
                        let length = random.next() as usize % 21;
                        let mut bytes = [0xabu8; 20];
                        let expected = length
                            .min(reader.content.description.bytes - reader.position)
                            .min(16 - reader.position % 16);
                        let count = store
                            .read(&mut reader.cursor, &mut bytes[..length])
                            .unwrap();
                        assert_eq!(count, expected);
                        assert_eq!(
                            &bytes[..count],
                            &reader.content.bytes[reader.position..reader.position + count]
                        );
                        assert!(bytes[count..].iter().all(|byte| *byte == 0xab));
                        reader.position += count;
                        assert_eq!(reader.cursor.position(), reader.position);
                        coverage.reads += count;
                    }
                    13 if readers[lease].is_some() => {
                        let reader = readers[lease].take().unwrap();
                        store.release_snapshot(reader.snapshot.id).unwrap();
                        let mut cursor = reader.cursor;
                        assert_eq!(store.read(&mut cursor, &mut [0; 16]), Err(Error::InvalidId));
                        assert_eq!(store.currentness(reader.snapshot.id), Err(Error::InvalidId));
                    }
                    14 => {
                        store.reclaim();
                    }
                    15 if iteration % 97 == 0 => {
                        store.remove(resource).unwrap();
                        assert_eq!(store.revision(resource), Err(Error::InvalidId));
                        let replacement = store.create().unwrap();
                        assert_ne!(replacement, resource);
                        resource = replacement;
                        revision = Revision(0);
                        active = None;
                        coverage.removals += 1;
                    }
                    _ => {}
                }
                for reader in readers.iter().flatten() {
                    let expected = if reader.content.resource != resource {
                        Currentness::ResourceRemoved
                    } else if reader.content.revision != revision {
                        Currentness::Stale
                    } else {
                        Currentness::Current
                    };
                    assert_eq!(store.currentness(reader.snapshot.id), Ok(expected));
                }
                if let Some(id) = stale[random.next() as usize % 16] {
                    assert!(matches!(
                        store.write_upload(id, 0, &[0]),
                        Err(Error::InvalidId | Error::InvalidState)
                    ));
                }
                let usage = store.usage();
                assert!(usage.reserved_pages <= usage.free_pages && usage.free_pages <= 32);
                assert!(usage.free_versions <= 8 && usage.free_snapshots <= 8);
                assert!(store.work().copied_bytes <= 16 && store.work().validation_steps <= 16);
                coverage.peak_versions = coverage.peak_versions.max(8 - usage.free_versions);
                coverage.peak_leases = coverage.peak_leases.max(8 - usage.free_snapshots);
            }
            for transfer in transfers.iter().flatten() {
                store.cancel_upload(transfer.id).unwrap();
            }
            for reader in readers.iter().flatten() {
                store.release_snapshot(reader.snapshot.id).unwrap();
            }
            store.remove(resource).unwrap();
            for _ in 0..64 {
                store.begin_callback();
                assert!(store.reclaim() <= 16);
            }
        });
        let usage = store.usage();
        assert_eq!(usage.free_pages, 32);
        assert_eq!(usage.reserved_pages, 0);
        assert_eq!(usage.free_versions, 8);
        assert_eq!(usage.free_snapshots, 8);
        assert_eq!(usage.retired_versions, 0);
        assert_eq!(usage.retired_pages, 0);
    }
    println!("resource state model: {coverage:?}");
    if !cfg!(miri) {
        assert!(
            coverage.commits > 100
                && coverage.reads > 1000
                && coverage.refusals > 1000
                && coverage.malformed > 100
                && coverage.removals > 10
        );
        assert_eq!(coverage.peak_versions, 8);
        assert!(coverage.peak_leases >= 6);
    }
}
