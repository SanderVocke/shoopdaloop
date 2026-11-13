//! Isolated table-indexing property with two independent delayed metadata slaves.
use crate::{
    Result,
    bits::Bits,
    clock::Edges,
    model::{Design, Model},
    random::Random,
    sim::{Agent, Sim},
};
use std::{cell::RefCell, rc::Rc};
#[derive(Clone)]
struct View {
    chunks: Vec<u64>,
    range: bool,
}
struct Metadata {
    views: Vec<Option<View>>,
    cf: u64,
    slot_bits: u32,
    random: Random,
    pending: [Option<(u64, Bits)>; 2],
    observed: Vec<u64>,
}
impl Agent for Metadata {
    fn drive(&mut self, m: &mut Model<'_>, e: Edges) {
        if !e.rises(0) {
            return;
        }
        m.set("prefetch_ready", u64::from(self.random.yes(4)));
        for (i, port) in ["descriptor", "table"].into_iter().enumerate() {
            m.set(
                &format!("{port}_ready"),
                u64::from(self.pending[i].is_none() && self.random.yes(3)),
            );
            m.set(
                &format!("{port}_response"),
                u64::from(
                    self.pending[i]
                        .as_ref()
                        .is_some_and(|(delay, _)| *delay == 0),
                ),
            );
            m.set_bits(
                &format!("{port}_data"),
                &self.pending[i]
                    .as_ref()
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default(),
            );
        }
    }
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        if !e.rises(0) {
            return;
        }
        if m.get("reset") != 0 {
            self.pending = [None, None];
            return;
        }
        for (i, port) in ["descriptor", "table"].into_iter().enumerate() {
            if m.get(&format!("{port}_response")) != 0 {
                self.pending[i] = None;
            } else if let Some((delay, _)) = &mut self.pending[i] {
                *delay = delay.saturating_sub(1);
            }
            if m.get(&format!("{port}_valid")) != 0 && m.get(&format!("{port}_ready")) != 0 {
                let address = m.get(&format!("{port}_address"));
                let value = if i == 0 {
                    let mut descriptor = Bits::zero(160);
                    if let Some(view) = &self.views[address as usize] {
                        descriptor
                            .set(0, 1, 1)
                            .set(1, 1, u64::from(view.range))
                            .set(64, 64, view.chunks.len() as u64 * self.cf)
                            .set(128, 32, view.chunks.len() as u64);
                    }
                    descriptor
                } else {
                    let view = address >> self.slot_bits;
                    let ordinal = address & ((1 << self.slot_bits) - 1);
                    Bits::from_u64(
                        self.views[view as usize].as_ref().unwrap().chunks[ordinal as usize],
                    )
                };
                self.pending[i] = Some((self.random.below(4), value));
            }
        }
        if m.get("prefetch_valid") != 0 && m.get("prefetch_ready") != 0 {
            self.observed.push(m.get("prefetch_chunk"));
        }
    }
}
pub fn resolver_conformance(c: [u64; 3]) -> Result<()> {
    let [views, slots, cf] = c;
    let compiled = Design::new(
        "media_prefetch_resolver",
        vec![crate::root().join("fpga/media_store/rtl/media_prefetch_resolver.sv")],
    )
    .parameters(&[("SLOTS", slots), ("CHUNK_FRAMES", cf)])
    .build(&crate::root().join("work/marlin-suite/resolver"))?;
    let mut rng = Random::new(20261111);
    let mut metadata = vec![Some(View {
        chunks: (0..slots).collect(),
        range: false,
    })];
    for _ in 0..views.min(6) - 2 {
        let count = 1 + rng.below(slots);
        let mut ids: Vec<_> = (0..slots).collect();
        for i in 0..slots as usize {
            let j = rng.below(slots) as usize;
            ids.swap(i, j);
        }
        ids.truncate(count as usize);
        metadata.push(Some(View {
            chunks: ids,
            range: true,
        }));
    }
    metadata.push(None);
    let mut requests = vec![(0, slots - 1, slots, 3)];
    for _ in 0..160 {
        let view = rng.below(metadata.len() as u64 + 2);
        let n = metadata
            .get(view as usize)
            .and_then(Option::as_ref)
            .map(|v| v.chunks.len() as u64)
            .unwrap_or(0);
        requests.push((
            view,
            rng.below(n + 2),
            rng.below(n + 2),
            rng.below(2 * slots + 1),
        ));
    }
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
    for (valid, ready, payload) in [
        ("descriptor_valid", "descriptor_ready", "descriptor_address"),
        ("table_valid", "table_ready", "table_address"),
        ("prefetch_valid", "prefetch_ready", "prefetch_chunk"),
    ] {
        s.agent(crate::protocol::ReadyValid::new(valid, ready, &[payload]));
    }
    let agent = Rc::new(RefCell::new(Metadata {
        views: metadata.clone(),
        cf,
        slot_bits: (64 - (slots - 1).leading_zeros()).max(1),
        random: rng,
        pending: [None, None],
        observed: Vec::new(),
    }));
    s.agent(agent.clone());
    s.model.set_all(&[
        ("reset", 1),
        ("view_count", metadata.len() as u64),
        ("slot_count", slots),
    ]);
    s.idle(1);
    s.model.set("reset", 0);
    for (view, position, length, count) in requests {
        let expected: Vec<_> = metadata
            .get(view as usize)
            .and_then(Option::as_ref)
            .filter(|v| length > 0 && length <= v.chunks.len() as u64 && position < length)
            .map(|v| {
                (0..count)
                    .map(|i| v.chunks[((position + i) % length) as usize])
                    .collect()
            })
            .unwrap_or_default();
        agent.borrow_mut().observed.clear();
        s.model.set_all(&[
            ("hint_valid", 1),
            ("hint_view", view),
            ("hint_position", position * cf),
            ("hint_length", length * cf),
            ("hint_count", count * cf),
        ]);
        let mut admitted = false;
        for _ in 0..10_000 {
            if s.tick(&["hint_ready"]).0.get("hint_ready") != 0 {
                admitted = true;
                break;
            }
        }
        assert!(admitted);
        s.model.set("hint_valid", 0);
        s.until(10_000, "resolver completion", |m| m.get("hint_ready") != 0);
        assert_eq!(
            agent.borrow().observed,
            expected,
            "view={view}, position={position}, length={length}, count={count}"
        );
    }
    Ok(())
}
