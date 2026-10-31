# 9. Loading and reading resources

<!-- rust-source: ../../examples/stored_audio.rs -->

[Contents](../README.md) · [Previous: Processing and scheduling MIDI](../part-2-control-a-running-graph/08-processing-and-scheduling-midi.md) · [Next: Recording and exporting](10-recording-and-exporting.md)

A `ResourceStore` owns fixed-page media storage. Uploading a new revision does not overwrite a revision held by a snapshot. A snapshot lease keeps its bytes and metadata readable until explicitly released.

```text
resource → active revision 2
snapshot → retained revision 1 → immutable pages
```

The store supplies bytes, not file decoding, resampling or playback DSP. Callback audio is planar; stored audio is interleaved little-endian `f32`.

## Run the example

The [stored audio example](../../examples/stored_audio.rs) uploads a known five-frame stereo pattern, pins it, publishes a replacement, and reads both:

```sh
cargo run -p rt_runtime --example stored_audio
```

It also checks required reads with no background copy allowance, page-boundary short reads, an owning executor Read reply, and complete reclamation. Data operations and its runtime callback are heap-guarded. Buffers, serialization and final destruction are outside those scopes.

## 1. Prepare enough space for both revisions

The example uses 16-byte pages, eight pages, and four slots in each resource identity/version/snapshot pool. Each 40-byte revision occupies three pages. Retaining both requires simultaneous space, not just room for the newest content.

Create a resource, then describe the upload:

```rust
    let description = Description {
        format: Format::Audio { channels: 2 },
        frames: 5,
        bytes: bytes.len(),
    };
    // Begin an upload: describe format and byte length; this reserves pages but does not publish.
    let upload = store.begin_upload(resource, expected, description)?;
```

A new resource has revision zero and no active content. Audio descriptions require nonzero channels and exactly `frames × channels × 4` bytes, with checked arithmetic. The example encodes the samples with `to_le_bytes`, not native struct layout. Sample-rate conversion is not implicit.

`begin_upload` checks the expected revision and reserves page credits. Replacement needs room for old/new versions, retained snapshots and any pages still awaiting reclamation.

## 2. Stream contiguous chunks, then publish

Write exactly the next byte offset. A chunk must fit the remaining current page and the current callback's copy allowance:

```rust
    for (index, chunk) in bytes.chunks(16).enumerate() {
        assert_no_heap(|| {
            store.begin_callback();
            store.write_upload(upload, index * 16, chunk)
        })?;
    }
    store.finish_upload(upload)?;
    assert_eq!(store.revision(resource)?, expected);
```

This direct-store example models one new host callback per chunk. A real direct integration calls `begin_callback` once per callback, never once per operation to escape its limit. When the store belongs to `GraphRuntime`, the executor owns that reset; processors cannot call it.

Finish checks completeness; it does **not** publish. `commit(upload)` checks the expected revision again and makes the finished version active. The example commits revisions 1 and 2. An intervening replacement can cause `StaleRevision`; cancel an upload you will not publish with `cancel_upload`.

For running-host uploads, use `Command::resource(Request::BeginUpload/Write/Finish/Commit)` and inspect their owning completions. Read/write buffers larger than `page_bytes` are refused before queue admission. MIDI upload chunks also need parser validation allowance, discussed in [Seeking and reading MIDI events](../part-3-work-with-stored-media/11-seeking-and-reading-midi-events.md).

## 3. Pin and read the intended revision

Acquire a snapshot before replacement, then derive a `ReadIndex`. After publishing the second revision, the example checks:

```rust
    assert_eq!(store.currentness(old.id)?, Currentness::Stale);
    assert_eq!(read_all(&mut store, old)?, original);
    assert_eq!(read_all(&mut store, newest)?, replacement);
    assert_eq!(store.render_io().snapshot(index)?, old);
```

`read_all` is an example helper: it allocates output off-render, then copies through a finite, progress-checked page loop. `RenderIo::snapshot(index)` returns metadata for that pinned prefix, not whatever is newest now.

Currentness describes the resource relationship:

| Status | Still-held snapshot |
|---|---|
| `Current` | Matches active content/status |
| `Stale` | Still readable after replacement, append or capture-status change |
| `ResourceRemoved` | Still readable after removal of the resource identity |

None permits using the snapshot after its lease is released. Raw `ResourceId`/`SnapshotId` values must come from this same store; coincident IDs from another store are not universally detected. Opaque indexes/cursors additionally validate store scope and lease lifetime.

A snapshot acquisition creates one lease. Copying its value, index or cursor creates none; dropping a plain copied handle releases none. These are local capabilities, not wire IDs for independent Wasm instances.

## 4. Keep required reads independent of optional transfer work

Legacy `store.read`/`ResourceIo::read` shares background copy allowance with commands and other optional work. The example spends all 16 bytes, then attempts another read:

```rust
        let mut required = store.cursor(old.id)?;
        assert_eq!(store.read(&mut required, &mut scratch), Err(Error::Budget));
        assert_eq!(required.position(), 0);
        assert_eq!(store.render_io().read(&mut required, &mut scratch)?, 16);
        assert_eq!(scratch, original[..16]);
        assert_eq!(store.copy_remaining(), 0);
```

Required `RenderIo` access still checks identities, shapes, offsets and physical storage. It counts work separately; it neither consumes nor renews optional allowance. In a processor, borrow it with `block.resources.render()`.

A raw read copies at most one page remainder. Starting at byte 4 returns 12 bytes even with a 16-byte destination: that is a page boundary, not EOF. Track `cursor.position()` against the immutable snapshot byte length; an empty output slice can also return zero. [Chapter 11](11-seeking-and-reading-midi-events.md) shows explicit positioning and complete MIDI event copies.

Required access cannot create resources/uploads/leases, begin or stop capture, reclaim, or reset accounting. Optional snapshot-tail copies remain budgeted. Prepared storage and independent work counters are not CPU-deadline guarantees.

## 5. Recover the cursor and owning Read buffer

The example moves the prepared store into `GraphRuntime::prepare_with_resources`, submits `Request::Read { cursor, bytes }`, and runs one callback with no signal graph. It checks `Outcome::Applied`, then the resource reply:

```rust
    assert_eq!(
        returned.reply(),
        Some(Reply::Read {
            written: 16,
            eof: false
        })
    );
    let Request::Read { cursor, bytes } = returned.into_request() else {
        panic!("not a read")
    };
    assert_eq!(cursor.position(), 16);
    assert_eq!(&bytes[..], &original[..16]);
```

The filled buffer and advanced cursor are in the returned request, not the Copy reply. Consume only `written` bytes; use `eof` rather than a short-copy guess. Reuse or dispose of the buffer off-render. A rejected/cancelled command still returns its request; Budget refusal is not an automatic retry.

## Release leases before reusing the pool

After callbacks stop, this NoPlanner example extracts the store with `into_resources`, releases both snapshots, removes the resource, and performs finite callback/reclamation steps. It asserts all pages, versions and snapshot slots are free. Released indexes then refuse access.

Release each acquired lease once, including cancelled exports. Removal/release can retire storage without making it immediately reusable. If cleanup depends on a planner's ownership ledger, extract it too with `into_planner_and_resources`; extraction itself settles nothing. [Chapter 19](../part-5-configure-and-operate-the-runtime/19-stopping-disconnecting-and-reclaiming-ownership.md) covers stopped cleanup.

[Next: Recording and exporting](10-recording-and-exporting.md)
