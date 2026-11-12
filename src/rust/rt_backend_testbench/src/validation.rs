use crate::spec::*;
use std::collections::BTreeSet;
pub type Checked<T> = std::result::Result<T, String>;

impl Scenario {
    pub fn validate(&self) -> Checked<()> {
        if self.format_version != FORMAT_VERSION {
            return Err("unsupported scenario format_version".into());
        }
        if self.name.is_empty()
            || !matches!(self.quantum, 1 | 16 | 32 | 64 | 128)
            || self.chunk_frames == 0
            || self.quantum % self.chunk_frames != 0
            || self.request_capacity == 0
        {
            return Err("invalid name, quantum, chunk size or request capacity".into());
        }
        fn identities(ids: impl Iterator<Item = Identity>) -> Checked<()> {
            let mut slots = BTreeSet::new();
            for id in ids {
                if id.generation == 0 || id.slot >= 65536 || !slots.insert(id.slot) {
                    return Err(format!("invalid or duplicate identity {id:?}"));
                }
            }
            Ok(())
        }
        identities(self.loops.iter().map(|l| l.id))?;
        identities(self.nodes.iter().map(|n| n.id))?;
        let mut media = BTreeSet::new();
        for m in &self.media {
            if m.name.is_empty()
                || !media.insert(&m.name)
                || m.channels.is_empty()
                || m.channels.iter().any(|c| {
                    c.len() != m.channels[0].len() || c.len() % self.chunk_frames as usize != 0
                })
            {
                return Err(format!("invalid media {}", m.name));
            }
        }
        for l in &self.loops {
            if l.channels == 0
                || l.sync_source
                    .is_some_and(|id| !self.loops.iter().any(|s| s.id == id))
            {
                return Err(format!("invalid channels/sync source for {:?}", l.id));
            }
            if let Some(c) = &l.content {
                let m = self
                    .media
                    .iter()
                    .find(|m| m.name == c.media)
                    .ok_or("unknown content media")?;
                let end = c
                    .start_chunks
                    .checked_add(c.length_chunks)
                    .and_then(|v| v.checked_mul(u64::from(self.chunk_frames)))
                    .ok_or("content range overflow")?;
                if m.channels.len() != l.channels as usize
                    || end > m.channels[0].len() as u64
                    || c.length_chunks == 0
                    || c.length_chunks * u64::from(self.chunk_frames) % u64::from(self.quantum) != 0
                {
                    return Err("invalid content range or channel count".into());
                }
            }
            let play = self
                .nodes
                .iter()
                .filter(|n| matches!(n.operation,NodeKind::Playback { target } if target==l.id))
                .count();
            let capture = self
                .nodes
                .iter()
                .filter(|n| matches!(n.operation,NodeKind::Capture { target } if target==l.id))
                .count();
            if play != 1 || capture != 1 {
                return Err(format!(
                    "loop {:?} needs exactly one playback and capture node",
                    l.id
                ));
            }
        }
        for n in &self.nodes {
            match n.operation {
                NodeKind::Playback { target } | NodeKind::Capture { target }
                    if !self.loops.iter().any(|l| l.id == target) =>
                {
                    return Err("node refers to unknown loop".into())
                }
                NodeKind::Gain { channels: 0, .. } => return Err("gain needs channels".into()),
                _ => {}
            }
        }
        for edge in &self.connections {
            for (ep, source) in [(edge.source, true), (edge.destination, false)] {
                let ports = if let Some(id) = ep.node {
                    let node = self
                        .nodes
                        .iter()
                        .find(|n| n.id == id)
                        .ok_or("connection refers to unknown node")?;
                    match node.operation {
                        NodeKind::Playback { target } if source => {
                            self.loops.iter().find(|l| l.id == target).unwrap().channels
                        }
                        NodeKind::Capture { target } if !source => {
                            self.loops.iter().find(|l| l.id == target).unwrap().channels
                        }
                        NodeKind::Gain { channels, .. } => channels,
                        _ => 0,
                    }
                } else if source {
                    self.input_ports
                } else {
                    self.output_ports
                };
                if ep.port >= ports {
                    return Err("connection refers to invalid port".into());
                }
            }
        }
        let mut names = BTreeSet::new();
        for step in &self.steps {
            if step.inputs.len() != self.input_ports as usize
                || step.inputs.iter().any(|c| c.len() != self.quantum as usize)
            {
                return Err("input channels must contain exactly one quantum".into());
            }
            for action in &step.actions {
                let name = match action {
                    Action::Transition {
                        name, operation, ..
                    } => {
                        let position = match operation {
                            Transition::Play { position, .. }
                            | Transition::Seek { position, .. } => *position,
                            _ => 0,
                        };
                        if position % u64::from(self.quantum) != 0 {
                            return Err("position is outside the aligned hardware profile".into());
                        }
                        Some(name)
                    }
                    Action::Gain { name, node, .. } => {
                        if !self
                            .nodes
                            .iter()
                            .any(|n| n.id == *node && matches!(n.operation, NodeKind::Gain { .. }))
                        {
                            return Err("gain control refers to unsupported node".into());
                        }
                        Some(name)
                    }
                    Action::Cancel { name, .. } | Action::Export { name, .. } => Some(name),
                };
                if let Some(name) = name {
                    if name.is_empty() || !names.insert(name) {
                        return Err("action names must be nonempty and unique".into());
                    }
                }
            }
        }
        Ok(())
    }
}
