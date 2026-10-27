//! Reuse bounded cancellation flags and owning upload chunks, including refusals.

use rt_runtime::buffer::{AudioBlock, MidiBlock};
use rt_runtime::executor::{Command, Control, GraphRuntime};
use rt_runtime::graph::PortCounts;
use rt_runtime::processor::{ProcessBlock, Processor};
use rt_runtime::resource::{Description, Format, Request, ResourceStore, UploadId};
use rt_runtime::transport::{Lane, Outcome};
use rt_runtime::{Budgets, Config, Error, Revision};
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
fn write(upload: UploadId, offset: usize, bytes: Box<[u8]>) -> Command<Unused> {
    Command::resource(Request::Write {
        upload,
        offset,
        bytes,
    })
}
fn buffer(command: Command<Unused>) -> Box<[u8]> {
    let Command::Resource(command) = command else {
        panic!("missing resource owner")
    };
    let Request::Write { bytes, .. } = command.into_request() else {
        panic!("missing chunk owner")
    };
    bytes
}
fn refused(control: &mut Control<Unused>, command: Command<Unused>, error: Error) -> Box<[u8]> {
    let refusal = control
        .submit_prepared(Lane::Normal, command)
        .err()
        .unwrap();
    assert_eq!(refusal.error, error);
    buffer(refusal.value)
}
fn render(runtime: &mut GraphRuntime<Unused>) -> Result<(), Error> {
    let mut output = [99.0];
    runtime.process(
        AudioBlock {
            frame: runtime.next_frame(),
            frames: 1,
            inputs: &[],
            outputs: &mut [&mut output],
        },
        MidiBlock {
            inputs: &[],
            outputs: &mut [],
        },
    )?;
    assert_eq!(output, [0.0]);
    Ok(())
}
fn main() -> Result<(), Error> {
    let config = Config {
        max_frames: 1,
        max_nodes: 1,
        max_ports: 1,
        max_edges: 1,
        audio_inputs: 0,
        audio_outputs: 1,
        midi_inputs: 0,
        midi_outputs: 0,
        command_slots: 1,
        scheduled_events: 1,
        resource_slots: 2,
        resource_pages: 4,
        page_bytes: 16,
        budgets: Budgets {
            commands: 2,
            events: 1,
            publications: 2,
            transfer_bytes: 16,
            ..Budgets::default()
        },
        ..Config::default()
    }
    .prepare()?;
    let mut store = ResourceStore::new(config)?;
    let resource = store.create()?;
    let upload = store.begin_upload(
        resource,
        Revision(0),
        Description {
            format: Format::Audio { channels: 1 },
            frames: 4,
            bytes: 16,
        },
    )?;
    let (mut control, mut runtime) =
        GraphRuntime::<Unused>::prepare_with_resources(config, vec![], store)?;
    let bytes = vec![0; 4].into_boxed_slice();
    let pointer = bytes.as_ptr();
    let spare = vec![0; 4].into_boxed_slice();
    let spare_pointer = spare.as_ptr();
    let bytes = refused(&mut control, write(upload, 0, bytes), Error::InvalidState);
    assert_eq!(bytes.as_ptr(), pointer);
    control.prepare_submissions()?;

    let (bytes, spare) = assert_no_heap(|| {
        let ticket = control
            .submit_prepared(Lane::Normal, write(upload, 0, bytes))
            .unwrap_or_else(|refusal| panic!("unexpected admission failure: {}", refusal.error));
        assert_eq!(control.prepare_submissions(), Err(Error::Busy));
        let spare = refused(&mut control, write(upload, 0, spare), Error::Full);
        ticket.cancel();
        render(&mut runtime).unwrap();
        let mut spare = refused(&mut control, write(upload, 0, spare), Error::Full);
        let mut bytes = {
            let completion = control.try_completion().unwrap();
            assert_eq!(completion.sequence, ticket.sequence());
            assert_eq!(completion.outcome, Outcome::Cancelled);
            assert_eq!(control.outstanding(), 0);
            spare = refused(&mut control, write(upload, 0, spare), Error::Busy);
            drop(ticket);
            spare = refused(&mut control, write(upload, 0, spare), Error::Busy);
            let recovery = control
                .submit_prepared(Lane::Recovery, Command::ResetMidi)
                .unwrap_or_else(|refusal| panic!("unexpected recovery failure: {}", refusal.error));
            let full = control
                .submit_prepared(Lane::Recovery, Command::ResetMidi)
                .err()
                .unwrap();
            assert_eq!(full.error, Error::Full);
            assert!(matches!(full.value, Command::ResetMidi));
            render(&mut runtime).unwrap();
            let recovered = control.try_completion().unwrap();
            assert_eq!(recovered.sequence, recovery.sequence());
            assert_eq!(recovered.outcome, Outcome::Applied);
            drop((recovered, recovery));
            buffer(completion.request)
        };
        assert_eq!(bytes.as_ptr(), pointer);
        assert_eq!(spare.as_ptr(), spare_pointer);
        for (index, sample) in [0.125f32, 0.25, 0.5, 1.0].into_iter().enumerate() {
            bytes.copy_from_slice(&sample.to_le_bytes());
            let ticket = control
                .submit_prepared(Lane::Normal, write(upload, index * 4, bytes))
                .unwrap_or_else(|refusal| panic!("unexpected reuse failure: {}", refusal.error));
            render(&mut runtime).unwrap();
            ticket.cancel();
            bytes = {
                let completion = control.try_completion().unwrap();
                assert_eq!(completion.sequence, ticket.sequence());
                assert_eq!(completion.outcome, Outcome::Applied);
                buffer(completion.request)
            };
            assert_eq!(bytes.as_ptr(), pointer);
            assert_eq!(bytes.len(), 4);
            assert_eq!(runtime.diagnostics().transfer_bytes, 4);
            drop(ticket);
        }
        assert_eq!(control.outstanding(), 0);
        assert!(control.try_execution().is_none());
        (bytes, spare)
    });
    assert_eq!(bytes.as_ptr(), pointer);
    assert_eq!(spare.as_ptr(), spare_pointer);
    let end = runtime.next_frame();
    let mut store = runtime.into_resources();
    store.finish_upload(upload)?;
    assert_eq!(store.commit(upload)?, Revision(1));
    let snapshot = store.snapshot(resource, end)?;
    let mut cursor = store.cursor(snapshot.id)?;
    let mut recorded = [0; 16];
    assert_no_heap(|| store.render_io().read(&mut cursor, &mut recorded))
        .map(|count| assert_eq!(count, 16))?;
    for (index, sample) in [0.125f32, 0.25, 0.5, 1.0].into_iter().enumerate() {
        assert_eq!(&recorded[index * 4..index * 4 + 4], &sample.to_le_bytes());
    }
    store.release_snapshot(snapshot.id)?;
    store.remove(resource)?;
    store.begin_callback();
    store.reclaim();
    assert_eq!(store.usage().free_pages, 4);
    assert_eq!(store.usage().free_versions, 2);
    assert_eq!(store.usage().free_snapshots, 2);
    drop((bytes, spare, store, control));
    println!("prepared writes: Full versus Busy, independent recovery, cancellation cuts and identical chunk storage verified");
    Ok(())
}
