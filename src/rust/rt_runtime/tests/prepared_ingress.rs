mod support;
use rt_runtime::buffer::{AudioBlock, MidiBlock};
use rt_runtime::executor::{Command, GraphRuntime};
use rt_runtime::graph::*;
use rt_runtime::processor::{ProcessBlock, Processor};
use rt_runtime::resource::{Description, Format, Request, ResourceStore};
use rt_runtime::transport::{channel, Lane, Outcome, TransportConfig};
use rt_runtime::{Budgets, Config, Epoch, Error, Frame, NodeId, Revision};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[shoop_test]
fn prepared_submission_flags_never_reuse_live_tickets_or_borrow_recovery_capacity() {
    let (mut control, mut render) = channel::<u32, (), ()>(TransportConfig {
        normal_slots: 1,
        recovery_slots: 1,
        telemetry_slots: 1,
        operations_per_cycle: 2,
        recovery_per_cycle: 1,
        publications_per_cycle: 2,
    })
    .unwrap();
    assert_eq!(
        control.submit_prepared(Lane::Normal, 10).unwrap_err().error,
        Error::InvalidState
    );
    control.prepare_submissions().unwrap();
    let ticket = assert_no_heap(|| control.submit_prepared(Lane::Normal, 10)).unwrap();
    assert_no_heap(|| {
        assert_eq!(control.prepare_submissions(), Err(Error::Busy));
        ticket.cancel();
    });
    let completion = assert_no_heap(|| {
        render.begin_cycle(Frame(0), Epoch(0));
        assert!(render.next_operation().is_none());
        control.try_completion().unwrap()
    });
    assert_eq!(completion.outcome, Outcome::Cancelled);
    assert_no_heap(|| {
        drop(ticket);
        let refused = control.submit_prepared(Lane::Normal, 11).unwrap_err();
        assert_eq!(refused.error, Error::Busy);
        assert_eq!(refused.value, 11);
    });
    let recovery_ticket = assert_no_heap(|| control.submit_prepared(Lane::Recovery, 90)).unwrap();
    assert_no_heap(|| {
        render.begin_cycle(Frame(1), Epoch(0));
        assert_eq!(*render.next_operation().unwrap().request, 90);
        render.complete(Outcome::Applied, None).unwrap();
        drop(control.try_completion().unwrap());
        drop(completion);
        for index in 0..1000 {
            let ticket = control.submit_prepared(Lane::Normal, index).unwrap();
            render.begin_cycle(Frame(index as u64 + 2), Epoch(0));
            assert_eq!(*render.next_operation().unwrap().request, index);
            render.complete(Outcome::Applied, None).unwrap();
            let completion = control.try_completion().unwrap();
            assert_eq!(completion.outcome, Outcome::Applied);
            assert_eq!(
                control.submit_prepared(Lane::Normal, 12).unwrap_err().error,
                Error::Busy
            );
            ticket.cancel(); // Too late, and cannot affect the next reused flag.
            drop(completion);
            drop(ticket);
        }
        drop(recovery_ticket);
    });
    assert_eq!(control.outstanding(), 0);
}

struct Empty;
impl Processor for Empty {
    fn ports(&self) -> PortCounts {
        PortCounts::default()
    }
    fn process(&mut self, _: &mut ProcessBlock<'_>) -> Result<(), Error> {
        Ok(())
    }
}
#[shoop_test]
fn prepared_executor_ingress_recycles_owning_chunk_buffers_without_heap_operations() {
    let config = Config {
        max_frames: 4,
        audio_inputs: 0,
        audio_outputs: 1,
        midi_inputs: 0,
        midi_outputs: 0,
        resource_slots: 2,
        resource_pages: 4,
        page_bytes: 64,
        budgets: Budgets {
            transfer_bytes: 64,
            ..Budgets::default()
        },
        ..Config::default()
    }
    .prepare()
    .unwrap();
    let mut store = ResourceStore::new(config).unwrap();
    let resource = store.create().unwrap();
    let upload = store
        .begin_upload(
            resource,
            Revision(0),
            Description {
                format: Format::Audio { channels: 1 },
                frames: 64,
                bytes: 256,
            },
        )
        .unwrap();
    let (mut control, mut runtime) =
        GraphRuntime::<Empty>::prepare_with_resources(config, vec![], store).unwrap();
    control.prepare_submissions().unwrap();
    let mut bytes = vec![0; 4].into_boxed_slice();
    let pointer = bytes.as_ptr();
    bytes = assert_no_heap(|| {
        let mut buffer = bytes;
        for index in 0..64 {
            buffer[0] = index as u8;
            let ticket = control
                .submit_prepared(
                    Lane::Normal,
                    Command::resource(Request::Write {
                        upload,
                        offset: index * 4,
                        bytes: buffer,
                    }),
                )
                .map_err(|e| e.error)
                .unwrap();
            let frame = runtime.next_frame();
            let mut output = [0.; 4];
            runtime
                .process(
                    AudioBlock {
                        frame,
                        frames: 4,
                        inputs: &[],
                        outputs: &mut [&mut output],
                    },
                    MidiBlock {
                        inputs: &[],
                        outputs: &mut [],
                    },
                )
                .unwrap();
            let completion = control.try_completion().unwrap();
            assert_eq!(completion.outcome, Outcome::Applied);
            let Command::Resource(command) = completion.request else {
                panic!("missing owning request");
            };
            let Request::Write { bytes, .. } = command.into_request() else {
                panic!("missing chunk");
            };
            buffer = bytes;
            assert_eq!(buffer.as_ptr(), pointer);
            assert_eq!(runtime.diagnostics().transfer_bytes, 4);
            drop(ticket);
        }
        buffer
    });
    assert_eq!(bytes.as_ptr(), pointer);
    assert_eq!(control.outstanding(), 0);
}

#[shoop_test]
fn reusable_plans_materialize_incrementally_without_allocating_or_bypassing_validation() {
    let config = Config {
        max_nodes: 2,
        max_ports: 4,
        max_edges: 2,
        audio_inputs: 0,
        audio_outputs: 1,
        midi_inputs: 0,
        midi_outputs: 0,
        ..Config::default()
    }
    .prepare()
    .unwrap();
    let first = NodeId::from_parts(0, 1).unwrap();
    let second = NodeId::from_parts(1, 1).unwrap();
    let catalog = Catalog::new(
        config,
        &[
            NodeSpec {
                id: first,
                ports: PortCounts {
                    audio_outputs: 1,
                    ..PortCounts::default()
                },
            },
            NodeSpec {
                id: second,
                ports: PortCounts {
                    audio_outputs: 1,
                    ..PortCounts::default()
                },
            },
        ],
    )
    .unwrap();
    let edge = |node| Connection {
        source: Endpoint {
            node: Some(node),
            kind: PortType::Audio,
            port: 0,
        },
        destination: Endpoint {
            node: None,
            kind: PortType::Audio,
            port: 0,
        },
    };
    let mut plan = PreparedPlan::with_capacity(config).unwrap();
    let mut validator = PlanValidator::new(config);
    assert_no_heap(|| {
        for _ in 0..100 {
            plan.begin_descriptor().unwrap();
            assert!(!plan.contains(first));
            assert!(!plan.contains(second));
            for node in [first, second] {
                plan.push_step(NodeStep { node, input_end: 0 }).unwrap();
            }
            assert_eq!(
                plan.push_step(NodeStep {
                    node: first,
                    input_end: 0
                }),
                Err(Error::Capacity)
            );
            plan.push_connection(edge(first)).unwrap();
            plan.push_connection(edge(second)).unwrap();
            assert_eq!(plan.push_connection(edge(first)), Err(Error::Capacity));
            assert!(plan.contains(first));
            assert!(plan.contains(second));
            validator.begin();
            let mut status = ValidationStatus::Pending;
            for _ in 0..32 {
                let progress = validator.advance(&plan, &catalog, 1);
                assert!(progress.steps <= 1);
                status = progress.status;
                if status != ValidationStatus::Pending {
                    break;
                }
            }
            assert_eq!(status, ValidationStatus::Valid);
        }
        plan.begin_descriptor().unwrap();
        plan.push_step(NodeStep {
            node: first,
            input_end: usize::MAX,
        })
        .unwrap();
        validator.begin();
        let mut status = ValidationStatus::Pending;
        for _ in 0..32 {
            status = validator.advance(&plan, &catalog, 1).status;
            if status != ValidationStatus::Pending {
                break;
            }
        }
        assert!(matches!(status, ValidationStatus::Invalid(_)));
        let huge = Config {
            max_edges: i32::MAX as usize,
            ..config.limits()
        }
        .prepare()
        .unwrap();
        assert!(matches!(
            PreparedPlan::with_capacity(huge),
            Err(Error::Capacity)
        ));
    });
    let mut compact = PreparedPlan::from_descriptor(config, PlanDescriptor::default()).unwrap();
    assert_no_heap(|| {
        assert_eq!(
            compact.push_step(NodeStep {
                node: first,
                input_end: 0
            }),
            Err(Error::Capacity)
        )
    });
}
