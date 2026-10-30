//! Upload immutable audio revisions, read pinned bytes and recover owning replies.

use rt_runtime::buffer::{AudioBlock, MidiBlock};
use rt_runtime::executor::{Command, GraphRuntime, PreparedNode};
use rt_runtime::graph::PortCounts;
use rt_runtime::processor::{ProcessBlock, Processor};
use rt_runtime::resource::{
    Currentness, Description, Format, Reply, Request, ResourceStore, Snapshot, UploadId,
};
use rt_runtime::transport::{Lane, Outcome};
use rt_runtime::{Budgets, Config, Error, Frame, ResourceId, Revision};
use shoop_wasm_test_support::heap::{assert_no_heap, TrackingAllocator};

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

struct Unused;
impl Processor for Unused {
    fn ports(&self) -> PortCounts {
        PortCounts::default()
    }
    fn process(&mut self, _: &mut ProcessBlock<'_>) -> Result<(), Error> {
        Ok(())
    }
}

fn upload(
    store: &mut ResourceStore,
    resource: ResourceId,
    expected: Revision,
    bytes: &[u8],
) -> Result<UploadId, Error> {
    let description = Description {
        format: Format::Audio { channels: 2 },
        frames: 5,
        bytes: bytes.len(),
    };
// Begin an upload: describe format and byte length; this reserves pages but does not publish.
    let upload = store.begin_upload(resource, expected, description)?;
    for (index, chunk) in bytes.chunks(16).enumerate() {
        assert_no_heap(|| {
            store.begin_callback();
            store.write_upload(upload, index * 16, chunk)
        })?;
    }
    store.finish_upload(upload)?;
    assert_eq!(store.revision(resource)?, expected);
    Ok(upload)
}

fn read_all(store: &mut ResourceStore, snapshot: Snapshot) -> Result<Vec<u8>, Error> {
    let mut bytes = vec![0; snapshot.description.bytes];
    let mut cursor = store.cursor(snapshot.id)?;
    assert_no_heap(|| {
        for _ in 0..snapshot.description.bytes.div_ceil(16) + 1 {
            let position = cursor.position();
            if position == bytes.len() {
                break;
            }
            let count = store
                .render_io()
                .read(&mut cursor, &mut bytes[position..])?;
            assert!(count > 0);
        }
        assert_eq!(cursor.position(), bytes.len());
        Ok::<_, Error>(())
    })?;
    Ok(bytes)
}

fn main() -> Result<(), Error> {
    let config = Config {
        max_frames: 8,
        audio_inputs: 0,
        audio_outputs: 0,
        midi_inputs: 0,
        midi_outputs: 0,
        max_nodes: 1,
        max_ports: 1,
        max_edges: 1,
        resource_slots: 4,
        resource_pages: 8,
        page_bytes: 16,
        budgets: Budgets {
            transfer_bytes: 16,
            validation_steps: 16,
            ..Budgets::default()
        },
        ..Config::default()
    }
    .prepare()?;
    let mut store = ResourceStore::new(config)?;
    let resource = store.create()?;
    assert_eq!(store.revision(resource)?, Revision(0));
    let samples: [f32; 10] = [1.0, -1.0, 0.5, -0.5, 0.25, -0.25, 2.0, -2.0, 0.0, 0.125];
    let original: Vec<u8> = samples
        .iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect();
    let replacement: Vec<u8> = samples
        .iter()
        .flat_map(|sample| (-sample).to_le_bytes())
        .collect();
    let first = upload(&mut store, resource, Revision(0), &original)?;
// commit makes the finished revision active; finish alone only checks completeness.
    assert_eq!(store.commit(first)?, Revision(1));
// Pin the current revision: the snapshot stays readable even after replacement.
    let old = store.snapshot(resource, Frame(0))?;
    let index = store.read_index(old.id)?;
    let second = upload(&mut store, resource, Revision(1), &replacement)?;
    assert_eq!(store.commit(second)?, Revision(2));
    let newest = store.snapshot(resource, Frame(1))?;
// After publishing revision 2, the pinned revision 1 is stale but still readable.
    assert_eq!(store.currentness(old.id)?, Currentness::Stale);
    assert_eq!(read_all(&mut store, old)?, original);
    assert_eq!(read_all(&mut store, newest)?, replacement);
    assert_eq!(store.render_io().snapshot(index)?, old);

    let mut scratch = [0xa5; 16];
    assert_no_heap(|| {
        store.begin_callback();
        let mut spent = store.cursor(old.id)?;
        assert_eq!(store.read(&mut spent, &mut scratch)?, 16);
        assert_eq!(store.copy_remaining(), 0);
// Required render-side reads use a separate counter and are not throttled by optional copy budget.
        let mut required = store.cursor(old.id)?;
        assert_eq!(store.read(&mut required, &mut scratch), Err(Error::Budget));
        assert_eq!(required.position(), 0);
        assert_eq!(store.render_io().read(&mut required, &mut scratch)?, 16);
        assert_eq!(scratch, original[..16]);
        assert_eq!(store.copy_remaining(), 0);
        let mut middle = store.render_io().cursor_at(index, 4)?;
        assert_eq!(store.render_io().read(&mut middle, &mut scratch)?, 12);
        assert_eq!(scratch[..12], original[4..16]);
        assert_eq!(middle.position(), 16);
        let mut eof = store.render_io().cursor_at(index, original.len())?;
        assert_eq!(store.render_io().read(&mut eof, &mut scratch)?, 0);
        assert!(matches!(
            store.render_io().cursor_at(index, original.len() + 1),
            Err(Error::InvalidOffset)
        ));
        Ok::<_, Error>(())
    })?;

    let cursor = store.cursor(old.id)?;
    let (mut control, mut runtime) =
        GraphRuntime::prepare_with_resources(config, Vec::<PreparedNode<Unused>>::new(), store)?;
    let ticket = control
        .submit(
            Lane::Normal,
            Command::resource(Request::Read {
                cursor,
                bytes: vec![0; 16].into_boxed_slice(),
            }),
        )
        .map_err(|refusal| refusal.error)?;
    assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame: Frame(0),
                frames: 8,
                inputs: &[],
                outputs: &mut [],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
    })?;
    let completion = control.try_completion().unwrap();
    assert_eq!(completion.sequence, ticket.sequence());
    assert_eq!(completion.outcome, Outcome::Applied);
    let Command::Resource(returned) = completion.request else {
        panic!("not a resource return")
    };
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
    drop(bytes);
    assert_eq!(control.outstanding(), 0);
    let mut store = runtime.into_resources();
    drop(control);
    store.release_snapshot(old.id)?;
    store.release_snapshot(newest.id)?;
    assert!(matches!(
        store.render_io().cursor_at(index, 0),
        Err(Error::InvalidId)
    ));
    store.remove(resource)?;
    for _ in 0..16 {
        assert_no_heap(|| {
            store.begin_callback();
            store.reclaim();
        });
    }
    assert_eq!(store.usage().free_pages, 8);
    assert_eq!(store.usage().free_versions, 4);
    assert_eq!(store.usage().free_snapshots, 4);
    assert_eq!(store.usage().reserved_pages, 0);
    drop(store);
    println!("stored audio: old/new revisions, required reads, byte positions, owning replies and full reclamation verified");
    Ok(())
}
