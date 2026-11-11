use rt_fpga_reference::{
    chunk_memory::ChunkMemory,
    fixed::Fixed,
    storage::{ChunkId, MediaStore},
    Error,
};
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn chunk_spans_preserve_neighbors_and_survive_prefetch_fence_and_retirement() {
    let mut memory = ChunkMemory::from_samples(4, (0..12).map(Fixed).collect()).unwrap();
    memory.prefetch(ChunkId(1)).unwrap();
    memory
        .write(ChunkId(1), 1, &[Fixed(-7), Fixed(i32::MIN)])
        .unwrap();
    let mut output = [Fixed::ZERO; 4];
    memory.read(ChunkId(1), 0, &mut output).unwrap();
    assert_eq!(output, [Fixed(4), Fixed(-7), Fixed(i32::MIN), Fixed(7)]);
    memory.fence().unwrap();
    memory.retire(ChunkId(1)).unwrap();
    memory.write(ChunkId(1), 0, &[Fixed(99)]).unwrap();
    memory.read(ChunkId(1), 0, &mut output).unwrap();
    assert_eq!(output, [Fixed(99), Fixed(-7), Fixed(i32::MIN), Fixed(7)]);
    assert_eq!(
        &memory.samples()[8..],
        &[Fixed(8), Fixed(9), Fixed(10), Fixed(11)]
    );
}

#[shoop_test]
fn invalid_chunk_spans_are_checked_before_any_payload_changes() {
    assert!(matches!(ChunkMemory::new(0), Err(Error::InvalidShape)));
    assert!(matches!(
        ChunkMemory::from_samples(4, vec![Fixed(1)]),
        Err(Error::InvalidShape)
    ));
    let mut memory = ChunkMemory::from_samples(4, vec![Fixed(7); 4]).unwrap();
    for (chunk, offset, samples, error) in [
        (ChunkId(1), 0, vec![Fixed(8)], Error::InvalidView),
        (ChunkId(usize::MAX), 0, vec![], Error::InvalidView),
        (ChunkId(0), usize::MAX, vec![], Error::InvalidShape),
        (ChunkId(0), 3, vec![Fixed(8); 2], Error::InvalidShape),
    ] {
        assert_eq!(memory.write(chunk, offset, &samples), Err(error));
        assert_eq!(memory.samples(), &[Fixed(7); 4]);
    }
    memory.read(ChunkId(0), 4, &mut []).unwrap();
    memory.write(ChunkId(0), 4, &[]).unwrap();
    assert_eq!(memory.prefetch(ChunkId(1)), Err(Error::InvalidView));
    assert_eq!(memory.retire(ChunkId(1)), Err(Error::InvalidView));
}

#[shoop_test]
fn elastic_growth_preserves_samples_and_initializes_only_new_slots() {
    let mut memory = ChunkMemory::new(3).unwrap();
    memory.reserve_slots(20).unwrap();
    assert_eq!(memory.slot_count(), 0);
    memory.grow_zeroed(2).unwrap();
    memory.write(ChunkId(1), 1, &[Fixed(41)]).unwrap();
    memory.grow_zeroed(20).unwrap();
    assert_eq!(memory.samples()[4], Fixed(41));
    assert!(memory.samples()[6..].iter().all(|x| *x == Fixed::ZERO));
    assert_eq!(memory.grow_zeroed(1), Err(Error::InvalidShape));
    assert_eq!(memory.reserve_slots(usize::MAX), Err(Error::MemoryFull));
    assert_eq!(memory.slot_count(), 20);
}

#[shoop_test]
fn media_views_retain_chunk_memory_contents_until_the_last_reference_retires() {
    let mut media = MediaStore::new(4).unwrap();
    let source = media
        .insert(&[Fixed(1), Fixed(2), Fixed(3), Fixed(4)])
        .unwrap();
    let retained = media.slice(source, 1, 2).unwrap();
    media.clear(source).unwrap();
    assert_eq!(media.free_chunk_count(), 0);
    let mut output = [Fixed::ZERO; 4];
    media.read_loop(retained, 1, 2, &mut output).unwrap();
    assert_eq!(output, [Fixed(3), Fixed(2), Fixed(3), Fixed(2)]);
    media.remove(retained).unwrap();
    assert_eq!(media.free_chunk_count(), 1);
    media.append(source, &[Fixed(8)]).unwrap();
    media.fence().unwrap();
    assert_eq!(
        media.chunk_memory().samples(),
        &[Fixed(8), Fixed(2), Fixed(3), Fixed(4)]
    );
}

#[shoop_test]
fn job_lookahead_is_advisory_and_can_precede_publication_of_a_recording() {
    use rt_fpga_reference::jobs::{BufferId, Job, JobProcessor, JobQueue, Memory};
    use rt_fpga_reference::storage::ViewId;
    struct Observed {
        media: MediaStore,
        operations: Vec<&'static str>,
    }
    impl Memory for Observed {
        fn chunk_frames(&self) -> usize {
            4
        }
        fn read(
            &mut self,
            view: ViewId,
            position: u64,
            length: u64,
            output: &mut [Fixed],
        ) -> Result<(), Error> {
            self.operations.push("read");
            self.media.read(view, position, length, output)
        }
        fn write(&mut self, view: ViewId, samples: &[Fixed]) -> Result<(), Error> {
            self.operations.push("write");
            self.media.write(view, samples)
        }
        fn prefetch(&mut self, view: ViewId, position: u64, length: u64, chunks: u64) {
            self.operations.push("prefetch");
            self.media.prefetch(view, position, length, chunks);
        }
    }
    let mut memory = Observed {
        media: MediaStore::new(4).unwrap(),
        operations: Vec::new(),
    };
    let view = memory.media.create_recording().unwrap();
    let mut queue = JobQueue::new(4);
    queue
        .push(Job::Input {
            port: 0,
            destination: BufferId(0),
        })
        .unwrap();
    queue
        .push(Job::Capture {
            source: BufferId(0),
            view,
        })
        .unwrap();
    queue
        .push(Job::ReadView {
            view,
            position: 0,
            length: 1,
            destination: BufferId(1),
        })
        .unwrap();
    queue
        .push(Job::Output {
            source: BufferId(1),
            port: 0,
        })
        .unwrap();
    let input = [Fixed(1), Fixed(-2), Fixed(3), Fixed(-4)];
    let mut output = [Fixed::ZERO; 4];
    JobProcessor::new(4, 2)
        .run(&mut queue, &mut memory, &[&input], &mut [&mut output])
        .unwrap();
    assert_eq!(output, input);
    assert_eq!(memory.operations, ["prefetch", "write", "read"]);
}
