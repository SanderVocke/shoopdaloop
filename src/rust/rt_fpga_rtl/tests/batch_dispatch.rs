#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_rtl::{bits::Bits, host::registers::*, model::Design, sim::Sim};
use shoop_wasm_test_support::shoop_test;
struct Rig<'a> {
    sim: Sim<'a>,
    rows: Vec<u64>,
    pending: Option<(u64, bool)>,
    writes: Vec<(usize, u64, u64)>,
    frame: u64,
    ticks: u64,
    quanta: u64,
    pace: bool,
    stall: bool,
    bad_read: Option<usize>,
    bad_write: Option<usize>,
}
impl<'a> Rig<'a> {
    fn step(&mut self) {
        self.sim.model.set("sync_frame", self.frame);
        self.sim.model.set("cfg_ready", u64::from(!self.stall));
        self.sim
            .model
            .set("cfg_response_valid", u64::from(self.pending.is_some()));
        if let Some((value, error)) = self.pending.take() {
            self.sim
                .model
                .set_bits("cfg_read_data", &Bits::from_u64(value));
            self.sim.model.set("cfg_response_error", u64::from(error));
        }
        let cfg = self.sim.model.get("cfg_valid") != 0 && !self.stall;
        if cfg {
            let address = self.sim.model.get("cfg_address") as usize;
            let write = self.sim.model.get("cfg_write") != 0;
            if write && self.bad_write != Some(address) {
                let value = self.sim.model.bits("cfg_write_data").get(0, 64);
                self.rows[address] = value;
                self.writes.push((address, value, self.frame));
            }
            self.pending = Some((
                self.rows[address],
                if write {
                    self.bad_write == Some(address)
                } else {
                    self.bad_read == Some(address)
                },
            ));
        }
        let accepted =
            self.sim.model.get("quantum_ready") != 0 && self.sim.model.get("quantum_valid") != 0;
        self.sim.idle(1);
        self.ticks += 1;
        if accepted {
            self.sim.model.set("quantum_valid", 0);
        }
        if self.sim.model.get("quantum_done") != 0 {
            self.frame += 32;
            self.quanta += 1;
        }
        if self.pace && self.ticks % 256 == 0 {
            self.sim.model.set("quantum_valid", 1);
        }
    }
    fn idle(&mut self, n: usize) {
        for _ in 0..n {
            self.step();
        }
    }
    fn send(&mut self, op: u32, args: &[u32]) -> (u32, [u32; 12]) {
        let mut words = [0u32; 12];
        words[..args.len()].copy_from_slice(args);
        self.sim.model.set_bits("command_data", &Bits::from(words));
        self.sim.model.set_all(&[
            ("command_op", op.into()),
            ("command_tag", 91),
            ("command_valid", 1),
        ]);
        let mut accepted = false;
        for _ in 0..20000 {
            if self.sim.model.get("command_ready") != 0 {
                self.step();
                accepted = true;
                break;
            }
            self.step();
        }
        assert!(accepted);
        self.sim.model.set("command_valid", 0);
        for _ in 0..20000 {
            if self.sim.model.get("response_valid") != 0 {
                let code = self.sim.model.get("response_code") as u32;
                let data = self.sim.model.bits("response_data").array::<12>();
                self.sim.model.set("response_ready", 1);
                self.step();
                self.sim.model.set("response_ready", 0);
                return (code, data);
            }
            self.step();
        }
        panic!("no batch completion");
    }
    fn begin(&mut self, n: u32, revision: u64, deadline: Option<u64>) {
        let d = deadline.unwrap_or(0);
        assert_eq!(
            self.send(
                OP_BEGIN_BATCH,
                &[
                    revision as u32,
                    (revision >> 32) as u32,
                    self.frame as u32,
                    (self.frame >> 32) as u32,
                    n,
                    32,
                    u32::from(deadline.is_some()),
                    d as u32,
                    (d >> 32) as u32
                ]
            )
            .0,
            RESULT_OK
        );
    }
    fn patch(
        &mut self,
        address: u32,
        expected: u64,
        guard: u64,
        mask: u64,
        value: u64,
        rebase: bool,
    ) -> u32 {
        self.send(
            OP_STAGE_PATCH,
            &[
                address | (u32::from(rebase) << 16),
                expected as u32,
                (expected >> 32) as u32,
                guard as u32,
                (guard >> 32) as u32,
                mask as u32,
                (mask >> 32) as u32,
                value as u32,
                (value >> 32) as u32,
            ],
        )
        .0
    }
}
#[shoop_test(no_wasm = "Dispatcher batch validation, pacing and timing")]
fn batch_validation_precedes_all_writes_and_rebases_at_one_common_boundary() {
    let root = rt_fpga_rtl::root();
    let rtl = root.join("fpga/host_bridge/rtl");
    let compiled = Design::new(
        "host_dispatch",
        vec![rtl.join("host_registers.sv"), rtl.join("host_dispatch.sv")],
    )
    .parameters(&[("STATE_DEPTH", 64)])
    .build(&root.join("work/batch-dispatch"))
    .unwrap();
    for case in 0..16 {
        let mut sim = Sim::new(compiled.session(None).unwrap(), &[("clk", 10000, 0)]);
        sim.model.set_all(&[
            ("reset", 1),
            ("epoch", 7),
            ("active_revision", 9),
            ("run_ready", 1),
            ("done", 1),
        ]);
        sim.idle(2);
        sim.model.set("reset", 0);
        sim.idle(1);
        let mut r = Rig {
            sim,
            rows: vec![0; 1024],
            pending: None,
            writes: vec![],
            frame: 32,
            ticks: 0,
            quanta: 0,
            pace: false,
            stall: false,
            bad_read: None,
            bad_write: None,
        };
        if case == 0 {
            assert_eq!(r.send(OP_ENABLE_AUDIO, &[1]).0, RESULT_OK);
            r.pace = true;
        }
        assert_eq!(
            r.send(OP_BEGIN_BATCH, &[9, 0, 32, 0, 513, 32]).0,
            RESULT_INVALID
        );
        assert_eq!(
            r.send(OP_BEGIN_BATCH, &[9, 0, 32, 0, 32, 33]).0,
            RESULT_INVALID
        );
        assert_eq!(
            r.send(OP_BEGIN_BATCH, &[9, 0, 32, 0, 32, 1]).0,
            RESULT_INVALID
        );
        let count = if case == 11 { 512 } else { 32 };
        r.begin(
            count,
            if case == 2 { 8 } else { 9 },
            if case == 3 { Some(0) } else { None },
        );
        let base = r.frame;
        for i in 0..count {
            assert_eq!(
                r.patch(
                    if case == 11 { i } else { i * 16 + 10 },
                    0,
                    if case == 1 && i == 31 { u64::MAX } else { 0 },
                    u64::MAX,
                    base + u64::from(case == 8),
                    case != 11
                ),
                RESULT_OK
            );
            if case == 0 {
                r.idle(1000);
            }
        }
        if case == 1 {
            r.rows[31 * 16 + 10] = 1;
        }
        if case == 4 {
            assert_eq!(r.patch(0, 0, 0, 1, 1, false), RESULT_INVALID);
        }
        if case == 5 {
            r.stall = true;
        }
        if case == 6 || case == 8 {
            r.frame = u64::MAX;
        }
        if case == 9 {
            r.bad_write = Some(16 + 10);
        }
        if case == 10 {
            r.bad_read = Some(31 * 16 + 10);
        }
        if case == 7 {
            assert_eq!(r.send(OP_DISCARD_BATCH, &[]).0, RESULT_OK);
        }
        if case == 12 {
            let mut malformed = [0; 11];
            malformed[10] = 1;
            assert_eq!(r.send(OP_STAGE_PATCH, &malformed).0, RESULT_INVALID);
        }
        if case == 13 {
            assert_eq!(
                r.send(OP_BEGIN_BATCH, &[9, 0, 32, 0, 513, 32]).0,
                RESULT_INVALID
            );
        }
        if case == 14 {
            assert_eq!(r.send(OP_COMMIT_BATCH, &[1]).0, RESULT_INVALID);
        }
        if case == 15 {
            assert_eq!(r.send(OP_DISCARD_BATCH, &[1]).0, RESULT_INVALID);
        }
        r.pace = false;
        r.idle(10);
        let at = r.frame;
        let before = r.quanta;
        let (code, data) = r.send(OP_COMMIT_BATCH, &[]);
        if case == 0 {
            assert_eq!(code, RESULT_OK);
            assert!(before > 0);
            assert_eq!(r.quanta, before);
            assert_eq!(data[0], at as u32);
            assert!(data[5] < 16666);
            assert_eq!(r.writes.len(), 32);
            assert!(
                r.writes
                    .iter()
                    .all(|(_, value, frame)| *value == at && *frame == at)
            );
            assert_eq!(r.send(OP_COMMIT_BATCH, &[]).0, RESULT_REJECTED);
            assert_eq!(r.writes.len(), 32);
        } else if case == 5 {
            assert_eq!(code, RESULT_FAULT);
            assert_eq!(
                r.sim.model.get("fault_cause"),
                u64::from(FAULT_CONTROL_TIMEOUT)
            );
            assert!(r.writes.is_empty());
        } else if case == 11 {
            assert_eq!(code, RESULT_OK);
            assert_eq!(r.writes.len(), 512);
            assert!(data[5] < 16666);
        } else if case == 9 {
            assert_eq!(code, RESULT_FAULT);
            assert_eq!(r.sim.model.get("fault_cause"), u64::from(FAULT_PIPELINE));
            assert_eq!(r.sim.model.get("audio_enable"), 0);
            assert_eq!(r.writes.len(), 1);
        } else if case == 6 {
            assert_eq!(code, RESULT_OK);
            assert!(r.writes.iter().all(|(_, value, _)| *value == u64::MAX));
        } else {
            assert_eq!(code, RESULT_REJECTED);
            assert!(r.writes.is_empty());
            if case == 1 {
                assert_eq!(data[4], 3);
            }
            if case == 2 {
                assert_eq!(data[4], 2);
            }
            if case == 3 {
                assert_eq!(data[4], 1);
            }
        }
    }
}
