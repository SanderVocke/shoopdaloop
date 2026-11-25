use rt_fpga_compiler::{
    CompileContext, Identity, ProgramLimits, compile_with_chunk_frames, loading::Allocator,
};
use rt_fpga_rtl::{
    host::{
        Client, Error, Mailbox, RegisterBus, Upload,
        control::{ControlWindow, ProgramSession},
        registers::STATUS_FAULT,
    },
    pipeline::Geometry,
    sim::Sim,
    target::RtlTarget,
};
use shoop_rt_protocol::immediate::{Code, PlannedTransition, TransitionMode, TransitionPlan};
use std::cell::{Cell, RefCell};
#[path = "../../../../../src/rust/rt_fpga_rtl/tests/support/host_axi.rs"]
mod host_axi;
#[path = "../../../../../src/rust/rt_fpga_compiler/tests/support/mono.rs"]
mod mono;

struct Paced<'a, 'm> {
    sim: &'a RefCell<Sim<'m>>,
    gap: &'a Cell<usize>,
    accesses: &'a Cell<usize>,
}
impl RegisterBus for Paced<'_, '_> {
    type Error = u64;
    fn read(&mut self, offset: u32) -> Result<u32, u64> {
        self.sim.borrow_mut().idle(self.gap.get() as u64);
        self.accesses.set(self.accesses.get() + 1);
        host_axi::Axi(self.sim).read(offset)
    }
    fn write(&mut self, offset: u32, value: u32) -> Result<(), u64> {
        self.sim.borrow_mut().idle(self.gap.get() as u64);
        self.accesses.set(self.accesses.get() + 1);
        host_axi::Axi(self.sim).write(offset, value)
    }
}
fn timeout() -> impl FnMut() -> bool {
    let mut polls = 20000;
    move || {
        polls -= 1;
        polls > 0
    }
}
fn main() {
    let limits = ProgramLimits {
        instructions: 256,
        state_values: 64,
        loop_slots: 32,
        jobs: 128,
        buffers: 64,
        history_slots: 64,
        input_ports: 1,
        output_ports: 2,
        quantum: 32,
    };
    let geometry = Geometry::new([64, 1024, 32], 32);
    let models =
        [16666, 33332].map(|cycles| (cycles, host_axi::build_pipeline(&geometry, limits, cycles)));
    let loops = (0..17)
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
    for skip in [0, 1] {
        for (control, targets, delay) in [
            (16666, 1, 0),
            (16666, 15, 0),
            (16666, 16, 0),
            (33332, 16, 0),
            (16666, 16, 1),
            (16666, 1, 10),
            (16666, 16, 10),
            (16666, 1, 20),
            (16666, 16, 20),
            (33332, 16, 20),
        ] {
            let compiled = &models
                .iter()
                .find(|(cycles, _)| *cycles == control)
                .unwrap()
                .1;
            let mut model = Sim::new(compiled.session(None).unwrap(), &[("clk", 10000, 0)]);
            model.model.set_all(&[("reset", 1), ("epoch", 1)]);
            model.idle(2);
            model.model.set("reset", 0);
            model.idle(2);
            let sim = RefCell::new(model);
            let gap = Cell::new(0);
            let accesses = Cell::new(0);
            let mut client = Client::new(
                Mailbox::open(
                    Paced {
                        sim: &sim,
                        gap: &gap,
                        accesses: &accesses,
                    },
                    host_axi::pipeline_contract(),
                )
                .unwrap(),
            );
            for view in 0..geometry.views as u32 {
                assert_eq!(
                    client
                        .exchange(
                            rt_fpga_rtl::host::Request::store_word(1, view, 1),
                            timeout()
                        )
                        .unwrap()
                        .code,
                    0
                );
            }
            for (address, value) in [geometry.views as u32, 0, 0, geometry.blocks() as u32]
                .into_iter()
                .enumerate()
            {
                assert_eq!(
                    client
                        .exchange(
                            rt_fpga_rtl::host::Request::store_word(0, address as u32, value),
                            timeout()
                        )
                        .unwrap()
                        .code,
                    0
                );
            }
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
            let ids = (0..17)
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
            let ready = (0..10000)
                .find_map(|_| session.upload_step(timeout()).unwrap())
                .unwrap();
            session.activate(ready, timeout()).unwrap();
            gap.set(delay);
            accesses.set(0);
            let start = sim.borrow().cycles;
            let mut completed = 0;
            let result = (|| -> Result<(), Error<u64>> {
                let mut window = ControlWindow::begin(session.client(), 1, timeout())?;
                for slot in 1..=targets {
                    let source = rt_fpga_reference::transitions::LoopId {
                        slot: ids[0].slot,
                        generation: ids[0].generation,
                    };
                    let code = window.apply_transition(
                        ids[slot],
                        Some(source),
                        None,
                        TransitionPlan {
                            first: Some(PlannedTransition {
                                mode: TransitionMode::Recording,
                                skip,
                            }),
                            ..TransitionPlan::default()
                        },
                    )?;
                    assert!(
                        matches!(code, Code::Applied | Code::Scheduled),
                        "unexpected arming response: {code:?}"
                    );
                    completed += 1;
                }
                window.release()
            })();
            let cycles = sim.borrow().cycles - start;
            let fault = host_axi::Axi(&sim).read(8).unwrap() & STATUS_FAULT != 0;
            let expected_success = targets < 16 || (delay == 0 && (skip == 0 || control == 33332));
            assert_eq!(result.is_ok(), expected_success);
            assert_eq!(fault, !expected_success);
            assert_eq!(sim.borrow().model.get("audio_enable"), 0);
            assert_eq!(sim.borrow().model.get("quantum_frame"), 0);
            println!(
                "skip={skip} control_cycles={control} loops={targets} extra_cycles_per_mmio={delay} completed={completed} accesses={} cycles={cycles} duration_us={:.2} fault={fault} result={result:?}",
                accesses.get(),
                cycles as f64 / 100.
            );
        }
    }
}
