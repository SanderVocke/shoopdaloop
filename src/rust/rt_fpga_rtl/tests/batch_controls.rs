#![cfg(all(feature = "sim", feature = "rt-interface", not(target_arch = "wasm32")))]
use rt_fpga_compiler::{
    CompileContext, Identity, ProgramLimits, compile_with_chunk_frames, loading::Allocator,
};
use rt_fpga_rtl::{
    host::{
        Client, Mailbox, RegisterBus, Request, Upload,
        control::{LoopBatch, ProgramSession},
        registers::*,
    },
    pipeline::Geometry,
    sim::Sim,
    target::RtlTarget,
};
use shoop_rt_protocol::immediate::{Code, PlannedTransition, TransitionMode, TransitionPlan};
use shoop_wasm_test_support::shoop_test;
use std::cell::{Cell, RefCell};
#[path = "support/host_axi.rs"]
mod host_axi;
#[path = "../../rt_fpga_compiler/tests/support/mono.rs"]
mod mono;
struct Slow<'a, 'm> {
    sim: &'a RefCell<Sim<'m>>,
    gap: &'a Cell<u64>,
}
impl RegisterBus for Slow<'_, '_> {
    type Error = u64;
    fn read(&mut self, offset: u32) -> Result<u32, u64> {
        self.sim.borrow_mut().idle(self.gap.get());
        host_axi::Axi(self.sim).read(offset)
    }
    fn write(&mut self, offset: u32, value: u32) -> Result<(), u64> {
        self.sim.borrow_mut().idle(self.gap.get());
        host_axi::Axi(self.sim).write(offset, value)
    }
}
fn budget() -> impl FnMut() -> bool {
    let mut n = 100000;
    move || {
        n -= 1;
        n > 0
    }
}
fn words<B: RegisterBus>(client: &mut Client<B>, slot: u32) -> [u64; 16]
where
    B::Error: core::fmt::Debug,
{
    let snapshot = client
        .exchange(Request::capture_loop(slot), budget())
        .unwrap();
    assert_eq!(snapshot.code, RESULT_OK);
    let mut row = [0; 16];
    for (i, chunk) in row.chunks_mut(6).enumerate() {
        let reply = client
            .exchange(
                Request::read_snapshot(snapshot.data[0], i as u32 * 6, chunk.len() as u32),
                budget(),
            )
            .unwrap();
        assert_eq!(reply.code, RESULT_OK);
        for (word, data) in chunk.iter_mut().zip(reply.data.chunks_exact(2)) {
            *word = u64::from(data[0]) | u64::from(data[1]) << 32;
        }
    }
    row
}
#[shoop_test]
fn batch_preparation_bounds_targets_and_merges_repeated_edits() {
    let mut batch = LoopBatch::new();
    let source = Some(rt_fpga_reference::transitions::LoopId {
        slot: 0,
        generation: 7,
    });
    let plan = TransitionPlan {
        first: Some(PlannedTransition {
            mode: TransitionMode::Recording,
            skip: 1,
        }),
        ..Default::default()
    };
    let mut row = [0u64; 16];
    row[0] = 15;
    for _ in 0..32 {
        assert_eq!(
            batch.add_future(
                Identity {
                    slot: 1,
                    generation: 7
                },
                source,
                row,
                100,
                plan
            ),
            Ok(Code::Scheduled)
        );
    }
    for slot in 2..=32 {
        batch
            .add_future(
                Identity {
                    slot,
                    generation: 7,
                },
                source,
                row,
                100,
                plan,
            )
            .unwrap();
    }
    assert_eq!(
        batch.add_future(
            Identity {
                slot: 33,
                generation: 7
            },
            source,
            row,
            100,
            plan
        ),
        Err(Code::CannotApply)
    );
    batch.clear();
    assert_eq!(
        batch.add_future(
            Identity {
                slot: 1,
                generation: 8
            },
            source,
            row,
            100,
            plan
        ),
        Err(Code::TargetUnavailable)
    );
    assert_eq!(
        batch.add_future(
            Identity {
                slot: 4096,
                generation: 7
            },
            source,
            row,
            100,
            plan
        ),
        Err(Code::TargetUnavailable)
    );
    assert_eq!(
        batch.add_future(
            Identity {
                slot: 1,
                generation: 7
            },
            source,
            row,
            100,
            TransitionMode::Recording.into()
        ),
        Err(Code::Unsupported)
    );
}

#[shoop_test(no_wasm = "Real AXI pipeline batch timing and rejection")]
fn staged_16_and_32_loop_arming_is_bounded_under_slow_mmio_and_rejects_atomically() {
    let limits = ProgramLimits {
        instructions: 512,
        state_values: 128,
        loop_slots: 64,
        jobs: 256,
        buffers: 128,
        history_slots: 128,
        input_ports: 1,
        output_ports: 2,
        quantum: 32,
    };
    let geometry = Geometry::new([128, 1024, 32], 32);
    let compiled = host_axi::build_pipeline(&geometry, limits, 16666);
    for (count, gap, skip) in [(16, 0, 0), (16, 20, 1), (32, 0, 1), (32, 200, 1)] {
        let mut model = Sim::new(compiled.session(None).unwrap(), &[("clk", 10000, 0)]);
        model.model.set_all(&[("reset", 1), ("epoch", 1)]);
        model.idle(2);
        model.model.set("reset", 0);
        model.idle(2);
        let sim = RefCell::new(model);
        let delay = Cell::new(0);
        let mut client = Client::new(
            Mailbox::open(
                Slow {
                    sim: &sim,
                    gap: &delay,
                },
                host_axi::pipeline_contract(),
            )
            .unwrap(),
        );
        for view in 0..geometry.views as u32 {
            assert_eq!(
                client
                    .exchange(Request::store_word(1, view, 1), budget())
                    .unwrap()
                    .code,
                RESULT_OK
            );
        }
        for (address, value) in [geometry.views as u32, 0, 0, geometry.blocks() as u32]
            .into_iter()
            .enumerate()
        {
            assert_eq!(
                client
                    .exchange(Request::store_word(0, address as u32, value), budget())
                    .unwrap()
                    .code,
                RESULT_OK
            );
        }
        let loops = (0..=32)
            .map(|slot| mono::MonoLoop {
                id: Identity {
                    slot,
                    generation: 1,
                },
                source: (slot != 0).then_some(0),
                input: slot != 0,
                output: slot != 0,
                gain: 1.,
                repeat_sync: false,
            })
            .collect::<Vec<_>>();
        let program = compile_with_chunk_frames(
            &mono::session(&loops),
            CompileContext {
                quantum: 32,
                limits,
            },
            32,
        )
        .unwrap();
        let mut allocator = Allocator::new(limits);
        let layout = allocator.stage(&program).unwrap();
        let ids = (0..=32)
            .map(|slot| {
                layout
                    .loop_id(Identity {
                        slot,
                        generation: 1,
                    })
                    .unwrap()
            })
            .collect::<Vec<_>>();
        let views = layout.media_views(&program);
        let image = RtlTarget {
            capacities: limits,
            media_views: &views,
            state_base: 0,
        }
        .build_image(&program, layout)
        .unwrap();
        let mut session = ProgramSession::new(client);
        assert!(
            session
                .stage(Upload::new(image, limits.instructions))
                .is_ok()
        );
        let ready = (0..20000)
            .find_map(|_| session.upload_step(budget()).unwrap())
            .unwrap();
        session.activate(ready, budget()).unwrap();
        let client = session.client();
        let mut batch = LoopBatch::new();
        let source = Some(rt_fpga_reference::transitions::LoopId {
            slot: ids[0].slot,
            generation: ids[0].generation,
        });
        let plan = TransitionPlan {
            first: Some(PlannedTransition {
                mode: TransitionMode::Recording,
                skip,
            }),
            ..Default::default()
        };
        for id in &ids[1..=count] {
            batch
                .add_future(*id, source, words(client, id.slot), 0, plan)
                .unwrap();
        }
        delay.set(gap);
        assert_eq!(
            batch
                .commit(client, 1, 0, count as u32, None, budget())
                .unwrap(),
            (0, Code::Applied)
        );
        let held = batch.control_cycles().unwrap();
        assert!(held < 16666);
        println!("targets={count} skip={skip} extra_cycles_per_mmio={gap} control_cycles={held}");
        delay.set(0);
        for id in &ids[1..=count] {
            let row = words(client, id.slot);
            assert_eq!((row[1] >> 32) & 31, 11);
            assert_eq!(row[10], 0);
            assert_eq!(row[11], u64::from(skip));
            assert_eq!((row[0] >> 33) & 3, 0);
        }
        assert_eq!(
            host_axi::Axi(&sim).read(REG_STATUS).unwrap() & STATUS_FAULT,
            0
        );
        batch.clear();
        let replacement = TransitionPlan {
            first: Some(PlannedTransition {
                mode: TransitionMode::Stopped,
                skip: 3,
            }),
            ..Default::default()
        };
        for id in &ids[1..=count] {
            batch
                .add_future(*id, source, words(client, id.slot), 0, replacement)
                .unwrap();
        }
        let before = words(client, ids[1].slot);
        let last = ids[count];
        client
            .exchange(Request::write_loop_word(last.slot * 16, 0), budget())
            .unwrap();
        assert_eq!(
            batch
                .commit(client, 1, 0, count as u32, None, budget())
                .unwrap()
                .1,
            Code::CannotApply
        );
        assert_eq!(words(client, ids[1].slot), before);
        assert_eq!(
            batch
                .commit(client, 2, 0, count as u32, None, budget())
                .unwrap()
                .1,
            Code::StaleConfiguration
        );
    }
}
