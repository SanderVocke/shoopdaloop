use super::*;
use alloc::collections::BTreeSet;
use shoop_graph::{ChannelLayout, PortRole, SignalType};

pub(super) fn channels(layout: ChannelLayout) -> Result<u32, CompileError> {
    match layout {
        ChannelLayout::Direct { audio, midi: false } => Ok(audio),
        _ => Err(CompileError::InvalidProgram),
    }
}
fn finite_balance(value: f32) -> bool {
    value.is_finite() && (-1.0..=1.0).contains(&value)
}
fn layout(owner: Owner, value: ChannelLayout) -> Result<(), CompileError> {
    match value {
        ChannelLayout::Direct { midi: true, .. } | ChannelLayout::DryWet { dry_midi: true, .. } => {
            Err(unsupported(owner, Feature::Midi))
        }
        ChannelLayout::DryWet { .. } => Err(unsupported(owner, Feature::Processor)),
        _ => Ok(()),
    }
}
pub(super) fn graph(g: &Graph, context: CompileContext) -> Result<(), CompileError> {
    let invalid = CompileError::InvalidProgram;
    if !matches!(context.quantum, 1 | 16 | 32 | 64 | 128) {
        return Err(invalid);
    }
    if g.loops.len() as u64 > u64::from(context.limits.loop_slots) {
        return Err(CompileError::Capacity(Resource::Loops));
    }
    let mut owners = BTreeSet::new();
    let mut slots = BTreeSet::new();
    for owner in g
        .strips
        .iter()
        .map(|s| Owner::Strip(s.id))
        .chain(g.loops.iter().map(|l| Owner::Loop(l.id)))
        .chain(g.buses.iter().map(|b| Owner::Bus(b.id)))
    {
        let (kind, id) = match owner {
            Owner::Strip(id) => (0, id),
            Owner::Loop(id) => (1, id),
            Owner::Bus(id) => (2, id),
            _ => unreachable!(),
        };
        if id.generation == 0 || !owners.insert(owner) || !slots.insert((kind, id.slot)) {
            return Err(invalid);
        }
    }
    for s in &g.strips {
        let owner = Owner::Strip(s.id);
        layout(owner, s.layout)?;
        if s.processor.is_some() || s.default_dry_playback {
            return Err(unsupported(owner, Feature::Processor));
        }
        let c = s.controls;
        if !c.input_gain_db.is_finite()
            || !c.output_gain_db.is_finite()
            || !finite_balance(c.input_balance)
            || !finite_balance(c.output_balance)
        {
            return Err(invalid);
        }
        if c.automatic_monitoring {
            return Err(unsupported(owner, Feature::StripControls));
        }
    }
    let mut minimum_state = 0u64;
    for l in &g.loops {
        let owner = Owner::Loop(l.id);
        layout(owner, l.layout)?;
        if l.composite.is_some() {
            return Err(unsupported(owner, Feature::Composite));
        }
        if !l.gain.is_finite() || !(0.0..=1.0).contains(&l.gain) || !finite_balance(l.balance) {
            return Err(invalid);
        }
        if let Some(strip) = l.strip {
            if !g
                .strips
                .iter()
                .any(|s| s.id == strip && s.layout == l.layout)
            {
                return Err(invalid);
            }
        }
        if l.sync_source
            .is_some_and(|id| !owners.contains(&Owner::Loop(id)))
        {
            return Err(invalid);
        }
        minimum_state += u64::from(channels(l.layout)?) + 1;
        if minimum_state > u64::from(context.limits.state_values) {
            return Err(CompileError::Capacity(Resource::State));
        }
    }
    for b in &g.buses {
        if b.processor.is_some() {
            return Err(unsupported(Owner::Bus(b.id), Feature::Processor));
        }
        if !b.gain_db.is_finite() || !finite_balance(b.balance) {
            return Err(invalid);
        }
    }
    let mut ports = BTreeSet::new();
    let mut positions = BTreeSet::new();
    for p in &g.ports {
        if !ports.insert(p.id) || !positions.insert((p.id.owner, p.id.signal, p.id.role, p.channel))
        {
            return Err(invalid);
        }
        if !matches!(p.id.owner, Owner::Host | Owner::Global) && !owners.contains(&p.id.owner) {
            return Err(invalid);
        }
        if p.id.signal == SignalType::Midi {
            continue;
        }
        if !matches!(p.id.role, PortRole::Input | PortRole::Output) {
            return Err(unsupported(p.id.owner, Feature::Routing));
        }
        let count = match p.id.owner {
            Owner::Strip(id) => channels(g.strips.iter().find(|s| s.id == id).unwrap().layout)?,
            Owner::Loop(id) => channels(g.loops.iter().find(|s| s.id == id).unwrap().layout)?,
            Owner::Bus(id) => g.buses.iter().find(|b| b.id == id).unwrap().channels,
            Owner::Host => continue,
            Owner::Global => return Err(unsupported(p.id.owner, Feature::Routing)),
        };
        if p.channel >= count {
            return Err(invalid);
        }
    }
    let mut edges = BTreeSet::new();
    for e in &g.connections {
        if !ports.contains(&e.source)
            || !ports.contains(&e.destination)
            || !e.source.is_source()
            || e.destination.is_source()
            || e.source.signal != e.destination.signal
            || !edges.insert((e.source, e.destination))
        {
            return Err(invalid);
        }
        if e.source.signal == SignalType::Midi {
            return Err(unsupported(e.destination.owner, Feature::Midi));
        }
    }
    Ok(())
}
