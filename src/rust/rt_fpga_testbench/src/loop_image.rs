//! Compact SP loop RAM image and control-side request/event bookkeeping.
use crate::{bits::Bits, drivers::state::Program, sim::Sim};
use rt_fpga_reference::{
    state::{LoopSlot, PlannedTransition},
    transitions::{LoopSpec, Mode, Operation, Timing},
};
use std::collections::BTreeMap;

pub fn operation(op: Operation) -> (u64, u64, bool) {
    match op {
        Operation::Stop => (0, 0, false),
        Operation::Play {
            position,
            retrigger,
        } => (1, position, retrigger),
        Operation::Record => (2, 0, false),
        Operation::Seek {
            position,
            retrigger,
        } => (3, position, retrigger),
    }
}

pub fn image(slot: &LoopSlot, spec: &LoopSpec, stamp: Option<u64>) -> Bits {
    let mut b = Bits::zero(1024);
    let s = slot.state;
    assert_eq!(s.capture_revision, s.revision_watermark);
    assert_eq!(slot.render.content_length, s.content_length);
    b.set(0, 1, 1)
        .set(1, 32, u64::from(s.id.generation))
        .set(
            33,
            2,
            match s.mode {
                Mode::Stopped => 0,
                Mode::Playing => 1,
                Mode::Recording => 2,
            },
        )
        .set(35, 1, u64::from(s.repeat_sync));
    if let Some(source) = spec.sync_source {
        assert!(source.slot < 4096);
        b.set(36, 1, 1).set(37, 16, u64::from(source.slot)).set(
            64,
            32,
            u64::from(source.generation),
        );
    }
    let newer = match slot.planned {
        [Some(a), Some(b)] => b.command.sequence > a.command.sequence,
        [None, Some(_)] => true,
        _ => false,
    };
    b.set(53, 1, u64::from(slot.pulse_this_quantum))
        .set(54, 1, u64::from(slot.render.begin_capture))
        .set(55, 1, u64::from(stamp.is_some()))
        .set(56, 1, u64::from(newer));
    for (word, value) in [
        (2, s.position),
        (3, s.content_length),
        (4, s.capture_frames),
        (5, s.cycles),
        (6, s.content_revision),
        (7, s.revision_watermark),
        (8, slot.render.position),
        (9, stamp.unwrap_or(0)),
    ] {
        b.set(word * 64, 64, value);
    }
    for (i, plan) in slot.planned.iter().enumerate() {
        if let Some(p) = plan {
            let base = (10 + i * 3) * 64;
            let flags = 96 + i * 5;
            let (op, position, retrigger) = operation(p.command.operation);
            b.set(base, 64, p.effective)
                .set(base + 64, 64, p.remaining_ticks)
                .set(base + 128, 64, position)
                .set(flags, 1, 1)
                .set(
                    flags + 1,
                    1,
                    u64::from(matches!(p.command.timing, Timing::AfterTicks { .. })),
                )
                .set(flags + 2, 2, op)
                .set(flags + 4, 1, u64::from(retrigger));
        }
    }
    b
}

#[derive(Default)]
pub struct LoopControl {
    bindings: BTreeMap<u32, [Option<PlannedTransition>; 2]>,
    last_pulse: BTreeMap<u32, Option<u64>>,
    newer: BTreeMap<u32, u64>,
}
impl LoopControl {
    pub fn stage(&mut self, slot: &LoopSlot, spec: &LoopSpec, stamp: Option<u64>) -> Bits {
        let b = image(slot, spec, stamp);
        self.bindings.insert(spec.id.slot, slot.planned);
        self.last_pulse
            .entry(spec.id.slot)
            .or_insert(slot.state.last_pulse);
        self.newer.insert(spec.id.slot, b.get(56, 1));
        b
    }
    /// Resolve compact plan-slot acknowledgements using the admitted control records.
    pub fn observe(&mut self, events: &[Bits]) -> Vec<Bits> {
        events
            .iter()
            .map(|b| {
                let mut result = b.clone();
                let slot = b.get(32, 32) as u32;
                match b.get(0, 8) {
                    2 => {
                        *self.last_pulse.get_mut(&slot).unwrap() = Some(b.get(192, 64));
                    }
                    3 => {
                        let index = b.get(64, 64) as usize;
                        assert!(index < 2);
                        assert_eq!(b.get(128, 64), 0);
                        let plan = self.bindings.get_mut(&slot).unwrap()[index]
                            .take()
                            .expect("duplicate or unbound plan resolution");
                        result
                            .set(64, 32, u64::from(plan.request.slot))
                            .set(96, 32, u64::from(plan.request.generation))
                            .set(128, 64, plan.command.sequence);
                    }
                    _ => {}
                }
                result
            })
            .collect()
    }
    /// Check both physical execution state and metadata reconstructed from actual RTL events.
    pub fn verify(&self, actual: &Bits, slot: &LoopSlot, spec: &LoopSpec, stamp: u64) {
        assert_eq!(self.last_pulse[&spec.id.slot], slot.state.last_pulse);
        let mut expected = image(slot, spec, Some(stamp));
        expected.set(56, 1, self.newer[&spec.id.slot]);
        let mut actual = actual.clone();
        for (i, expected_plan) in slot.planned.iter().enumerate() {
            let binding = self.bindings[&spec.id.slot][i];
            assert_eq!(
                binding.map(|p| (p.request, p.command, p.effective)),
                expected_plan.map(|p| (p.request, p.command, p.effective))
            );
            if expected_plan.is_none() {
                assert_eq!(actual.get(96 + i * 5, 1), 0);
                actual.set(96 + i * 5, 5, 0);
                for word in 10 + i * 3..13 + i * 3 {
                    actual.set(word * 64, 64, 0);
                }
            }
        }
        assert_eq!(actual, expected, "loop {:?}, frame {stamp}", spec.id);
    }
}

pub fn access(program: &mut Program, s: &mut Sim<'_>, address: u64, value: Option<&Bits>) -> Bits {
    s.model.set("cfg_loop", 1);
    let result = program.configure(s, false, address, value, false);
    s.model.set("cfg_loop", 0);
    result
}
pub fn write_words(
    program: &mut Program,
    s: &mut Sim<'_>,
    slot: u32,
    row: &Bits,
    start: usize,
    end: usize,
) {
    for word in start..end {
        let mut b = Bits::zero(129);
        b.set(0, 64, row.get(word * 64, 64));
        access(program, s, u64::from(slot) * 16 + word as u64, Some(&b));
    }
}
pub fn write_plans(program: &mut Program, s: &mut Sim<'_>, slot: u32, row: &Bits) {
    let address = u64::from(slot) * 16;
    let mut flags = access(program, s, address, None);
    flags.set(56, 1, row.get(56, 1));
    access(program, s, address, Some(&flags));
    write_words(program, s, slot, row, 1, 2);
    write_words(program, s, slot, row, 10, 16);
}
pub fn read_loop(program: &mut Program, s: &mut Sim<'_>, slot: u32) -> Bits {
    let mut row = Bits::zero(1024);
    for word in 0..16 {
        let b = access(program, s, u64::from(slot) * 16 + word as u64, None);
        assert_eq!(b.get(64, 64), 0);
        assert_eq!(b.get(128, 1), 0);
        row.set(word * 64, 64, b.get(0, 64));
    }
    row
}
