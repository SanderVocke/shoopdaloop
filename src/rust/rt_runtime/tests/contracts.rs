mod support;

use rt_runtime::buffer::{AudioBlock, Event, EventBuffer};
use rt_runtime::{
    Config, Epoch, Error, Frame, NodeId, RenderRuntime, ResourceId, Revision, Sequence,
};
use shoop_wasm_test_support::shoop_test;
use std::alloc::{GlobalAlloc, Layout};
use std::cell::Cell;
use support::{assert_no_heap, measure, Counts, DropProbe};

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[shoop_test]
fn allocation_reallocation_and_deallocation_canaries_are_active() {
    let layout = Layout::from_size_align(32, 8).unwrap();
    // SAFETY: Each successful allocation is resized with its original layout and
    // released exactly once with the final matching layout.
    unsafe {
        let (pointer, counts) = measure(|| ALLOCATOR.alloc(layout));
        assert!(!pointer.is_null());
        assert_eq!(
            counts,
            Counts {
                allocations: 1,
                ..Counts::default()
            }
        );
        let (resized, counts) = measure(|| ALLOCATOR.realloc(pointer, layout, 64));
        assert!(!resized.is_null());
        assert_eq!(
            counts,
            Counts {
                reallocations: 1,
                ..Counts::default()
            }
        );
        let (_, counts) =
            measure(|| ALLOCATOR.dealloc(resized, Layout::from_size_align(64, 8).unwrap()));
        assert_eq!(
            counts,
            Counts {
                deallocations: 1,
                ..Counts::default()
            }
        );
        let (pointer, counts) = measure(|| ALLOCATOR.alloc_zeroed(layout));
        assert!(!pointer.is_null());
        assert_eq!(counts.allocations, 1);
        assert_eq!(*pointer, 0);
        ALLOCATOR.dealloc(pointer, layout);
    }
}

#[shoop_test]
fn guard_detects_a_real_collection_lifetime_and_destructor_location() {
    let realtime_drops = Cell::new(0);
    let control_drops = Cell::new(0);
    let (_, counts) = measure(|| {
        let bytes = std::hint::black_box(vec![1u8; 128]);
        std::hint::black_box(&bytes);
        drop(bytes);
        drop(DropProbe {
            realtime_drops: &realtime_drops,
            control_drops: &control_drops,
        });
    });
    assert!(counts.allocations > 0);
    assert!(counts.deallocations > 0);
    assert_eq!(realtime_drops.get(), 1);
    drop(DropProbe {
        realtime_drops: &realtime_drops,
        control_drops: &control_drops,
    });
    assert_eq!(control_drops.get(), 1);
}

#[shoop_test(no_wasm = "proves a heap violation fails a native test via unwind")]
#[should_panic(expected = "heap operation during rendering")]
fn heap_assertion_rejects_an_allocating_callback() {
    assert_no_heap(|| drop(std::hint::black_box(vec![1u8; 128])));
}

#[shoop_test(no_wasm = "unwind recovery is native-only; Wasm uses aborting panics")]
fn allocation_scope_is_reset_on_unwind() {
    assert!(std::panic::catch_unwind(|| measure(|| panic!("scope canary"))).is_err());
    assert!(!support::is_rendering());
    assert_no_heap(|| ());
}

#[shoop_test]
fn configuration_rejects_zeros_overflows_and_indivisible_work() {
    let base = Config::default();
    assert_no_heap(|| {
        assert_eq!(
            Config {
                sample_rate: 0,
                ..base
            }
            .prepare(),
            Err(Error::InvalidConfig)
        );
        assert_eq!(
            Config {
                max_frames: 0,
                ..base
            }
            .prepare(),
            Err(Error::InvalidConfig)
        );
        assert_eq!(
            Config {
                max_ports: usize::MAX,
                ..base
            }
            .prepare(),
            Err(Error::Capacity)
        );
        assert_eq!(
            Config {
                max_frames: i32::MAX as usize,
                ..base
            }
            .prepare(),
            Err(Error::Capacity)
        );
        assert!(Config {
            max_edges: 5_000_000,
            max_frames: 1024,
            scheduled_events: 1024,
            budgets: rt_runtime::Budgets {
                events: 1024,
                subranges: 1025,
                ..base.budgets
            },
            ..base
        }
        .prepare()
        .is_ok());
        assert!(Config {
            max_nodes: 1,
            max_ports: 1_000_000,
            max_edges: 16_000_000,
            midi_events_per_port: 1,
            midi_bytes_per_port: 1,
            scheduled_events: 128,
            budgets: rt_runtime::Budgets {
                events: 128,
                subranges: 129,
                ..base.budgets
            },
            ..base
        }
        .prepare()
        .is_ok());
        assert_eq!(
            Config {
                max_edges: 10_000,
                max_frames: 1024,
                scheduled_events: 1024,
                budgets: rt_runtime::Budgets {
                    events: 1024,
                    subranges: 1025,
                    ..base.budgets
                },
                ..base
            }
            .prepare()
            .unwrap()
            .limits()
            .max_edges,
            10_000
        );
        assert_eq!(
            Config {
                page_bytes: 3,
                ..base
            }
            .prepare(),
            Err(Error::InvalidConfig)
        );
        assert_eq!(
            Config {
                page_bytes: 8192,
                ..base
            }
            .prepare(),
            Err(Error::InvalidConfig)
        );
        assert_eq!(
            Config {
                audio_outputs: base.max_ports + 1,
                ..base
            }
            .prepare(),
            Err(Error::Capacity)
        );
    });
    let prepared = base.prepare().unwrap();
    assert_eq!(prepared.audio_samples(), base.max_frames * base.max_ports);
    assert_eq!(
        prepared.midi_bytes(),
        base.max_ports * base.midi_bytes_per_port
    );
    assert_eq!(
        prepared.resource_bytes(),
        base.resource_pages * base.page_bytes
    );
    let mut detached = prepared.limits();
    detached.max_frames = 1;
    assert_ne!(prepared.limits(), detached);
}

#[shoop_test]
fn every_positive_capacity_and_budget_is_validated() {
    type Field = fn(&mut Config) -> &mut usize;
    let fields: &[Field] = &[
        |c| &mut c.max_frames,
        |c| &mut c.max_nodes,
        |c| &mut c.max_ports,
        |c| &mut c.max_edges,
        |c| &mut c.midi_events_per_port,
        |c| &mut c.midi_bytes_per_port,
        |c| &mut c.scheduled_events,
        |c| &mut c.resource_slots,
        |c| &mut c.resource_pages,
        |c| &mut c.page_bytes,
        |c| &mut c.command_slots,
        |c| &mut c.budgets.commands,
        |c| &mut c.budgets.events,
        |c| &mut c.budgets.transfer_bytes,
        |c| &mut c.budgets.validation_steps,
        |c| &mut c.budgets.publications,
    ];
    for field in fields {
        let mut config = Config::default();
        *field(&mut config) = 0;
        assert_eq!(config.prepare(), Err(Error::InvalidConfig));
        *field(&mut config) = usize::MAX;
        assert_eq!(config.prepare(), Err(Error::Capacity));
    }
}

#[shoop_test]
fn counters_and_generations_exhaust_without_wrapping() {
    assert_no_heap(|| {
        assert_eq!(Frame(u64::MAX).checked_add(1), Err(Error::Exhausted));
        assert_eq!(Frame(u64::MAX - 1).checked_add(1), Ok(Frame(u64::MAX)));
        assert_eq!(Epoch(u64::MAX).checked_next(), Err(Error::Exhausted));
        assert_eq!(Revision(u64::MAX).checked_next(), Err(Error::Exhausted));
        assert_eq!(Sequence(u64::MAX).checked_next(), Err(Error::Exhausted));
        assert_eq!(Sequence(0).checked_next(), Ok(Sequence(1)));
        assert_eq!(NodeId::from_parts(0, 0), Err(Error::InvalidId));
        let old = NodeId::from_parts(17, 1).unwrap();
        let new = old.checked_next_generation().unwrap();
        assert_eq!((new.slot(), new.generation()), (17, 2));
        assert_ne!(old, new);
        assert_eq!(
            ResourceId::from_parts(0, u32::MAX)
                .unwrap()
                .checked_next_generation(),
            Err(Error::Exhausted)
        );
    });
}

#[shoop_test]
fn event_storage_is_bounded_ordered_and_refusal_is_atomic() {
    let mut events = [Event::default(); 3];
    let mut bytes = [0u8; 7];
    let mut buffer = EventBuffer::new(&mut events, &mut bytes).unwrap();
    assert_no_heap(|| {
        assert_eq!(buffer.push(8, 8, &[1]), Err(Error::InvalidEvent));
        assert_eq!(buffer.push(0, 8, &[]), Err(Error::InvalidEvent));
        buffer.push(1, 8, &[0x90, 60, 100]).unwrap();
        assert_eq!(buffer.push(0, 8, &[1]), Err(Error::InvalidEvent));
        buffer.push(1, 8, &[0x80, 60, 0]).unwrap();
        assert_eq!(buffer.push(2, 8, &[1, 2]), Err(Error::Full));
        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer.byte_len(), 6);
        assert_eq!(buffer.get(0), Some((1, &[0x90, 60, 100][..])));
        buffer.push(7, 8, &[0xf8]).unwrap();
        assert_eq!(buffer.push(7, 8, &[0xf8]), Err(Error::Full));
        assert_eq!(buffer.remaining_events(), 0);
        assert_eq!(buffer.remaining_bytes(), 0);
        assert_eq!(buffer.get(3), None);
        buffer.clear();
        assert!(buffer.is_empty());
        assert_eq!(buffer.byte_len(), 0);
        buffer.push(0, 1, &[0xf8]).unwrap();
        assert_eq!(buffer.get(0), Some((0, &[0xf8][..])));
    });
}

#[shoop_test]
fn empty_graph_renders_variable_prefixes_without_heap_operations() {
    let config = Config {
        max_frames: 8,
        audio_inputs: 1,
        audio_outputs: 1,
        ..Config::default()
    }
    .prepare()
    .unwrap();
    let mut runtime = RenderRuntime::new(config);
    let input = [1.0; 8];
    let mut output = [99.0; 8];
    for frames in [1, 8, 3, 7] {
        output.fill(99.0);
        let frame = runtime.next_frame();
        assert_no_heap(|| {
            runtime.process(AudioBlock {
                frame,
                frames,
                inputs: &[&input],
                outputs: &mut [&mut output],
            })
        })
        .unwrap();
        assert!(output[..frames].iter().all(|&sample| sample == 0.0));
        assert!(output[frames..].iter().all(|&sample| sample == 99.0));
    }
    assert_eq!(runtime.next_frame(), Frame(19));
    assert_eq!(runtime.config(), config);
}

#[shoop_test]
fn prepared_processor_uses_borrowed_disjoint_views_without_heap_operations() {
    use rt_runtime::processor::{PrepareProcessor, ProcessBlock, Processor};

    struct Gain(f32);
    impl PrepareProcessor for Gain {
        type Prepared = Self;
        fn prepare(self, _: rt_runtime::PreparedConfig) -> Result<Self, Error> {
            Ok(self)
        }
    }
    impl Processor for Gain {
        fn ports(&self) -> rt_runtime::graph::PortCounts {
            rt_runtime::graph::PortCounts {
                audio_inputs: 1,
                audio_outputs: 1,
                midi_inputs: 1,
                midi_outputs: 1,
            }
        }
        fn process(&mut self, block: &mut ProcessBlock<'_>) -> Result<(), Error> {
            let (input, output) = block.audio.pair(0, 0).ok_or(Error::InvalidShape)?;
            for (source, target) in input.iter().zip(output) {
                *target = *source * self.0;
            }
            for index in 0..block.midi_inputs[0].len() {
                let (offset, bytes) = block.midi_inputs[0].get(index).ok_or(Error::InvalidEvent)?;
                block.midi_outputs[0].push(offset, block.frames, bytes)?;
            }
            Ok(())
        }
    }
    let mut processor = Gain(0.5)
        .prepare(Config::default().prepare().unwrap())
        .unwrap();
    let input = [1.0, -2.0, 3.0, -4.0];
    let mut output = [0.0; 4];
    let mut midi_in = [rt_runtime::buffer::MidiBuffer::new(1, 3).unwrap()];
    let mut midi_out = [rt_runtime::buffer::MidiBuffer::new(1, 3).unwrap()];
    midi_in[0].push(2, 4, &[0x90, 60, 100]).unwrap();
    let mut resources =
        rt_runtime::resource::ResourceStore::new(Config::default().prepare().unwrap()).unwrap();
    resources.begin_callback();
    assert_no_heap(|| {
        processor.process(&mut ProcessBlock {
            frame: Frame(0),
            frames: 4,
            audio: rt_runtime::buffer::AudioPorts::new(&input, &mut output, 4, 0, 4).unwrap(),
            midi_inputs: &midi_in,
            midi_outputs: &mut midi_out,
            resources: resources.io(),
            decision: None,
            observations: rt_runtime::planner::ObservationWriter::disabled(&mut 0),
        })
    })
    .unwrap();
    assert_eq!(output, [0.5, -1.0, 1.5, -2.0]);
    assert_eq!(midi_out[0].get(0), Some((2, &[0x90, 60, 100][..])));
}

#[shoop_test]
fn zero_blocks_do_not_touch_buffers_validate_shapes_or_advance_clock() {
    let mut runtime = RenderRuntime::new(Config::default().prepare().unwrap());
    let mut output = [7.0; 2];
    assert_no_heap(|| {
        runtime.process(AudioBlock {
            frame: Frame(u64::MAX),
            frames: 0,
            inputs: &[],
            outputs: &mut [&mut output],
        })
    })
    .unwrap();
    assert_eq!(output, [7.0; 2]);
    assert_eq!(runtime.next_frame(), Frame(0));
}

#[shoop_test]
fn invalid_host_shapes_clear_only_a_bounded_writable_prefix() {
    let config = Config {
        max_frames: 4,
        audio_inputs: 0,
        audio_outputs: 1,
        ..Config::default()
    }
    .prepare()
    .unwrap();
    let mut runtime = RenderRuntime::new(config);
    let mut output = [9.0; 8];
    let mut extra = [9.0; 8];
    assert_eq!(
        assert_no_heap(|| runtime.process(AudioBlock {
            frame: Frame(0),
            frames: usize::MAX,
            inputs: &[],
            outputs: &mut [&mut output, &mut extra],
        })),
        Err(Error::InvalidShape)
    );
    assert_eq!(output, [0.0, 0.0, 0.0, 0.0, 9.0, 9.0, 9.0, 9.0]);
    assert_eq!(extra, [9.0; 8]);
    assert_eq!(runtime.next_frame(), Frame(0));
    let mut short = [9.0; 2];
    assert_eq!(
        assert_no_heap(|| runtime.process(AudioBlock {
            frame: Frame(0),
            frames: 4,
            inputs: &[],
            outputs: &mut [&mut short],
        })),
        Err(Error::InvalidShape)
    );
    assert_eq!(short, [0.0; 2]);
    assert_eq!(
        assert_no_heap(|| runtime.process(AudioBlock {
            frame: Frame(1),
            frames: 4,
            inputs: &[],
            outputs: &mut [&mut output],
        })),
        Err(Error::InvalidFrame)
    );
    assert_eq!(runtime.next_frame(), Frame(0));
    assert_no_heap(|| {
        runtime.process(AudioBlock {
            frame: Frame(0),
            frames: 4,
            inputs: &[],
            outputs: &mut [&mut output],
        })
    })
    .unwrap();
}
