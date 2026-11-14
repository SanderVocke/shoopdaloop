//! Immutable stage identities; the device retains readiness and commits actual activation epochs.
use crate::{audio_actions, audio_image::Transfer, protocol::put};
use rt_runtime::{Epoch, graph::activation::StageId};
use shoop_loops::{runtime::NodePairStageId, topology::staging};

fn configuration(opcode: u8) -> Transfer {
    let mut t = Transfer {
        opcode: 50,
        state: false,
        address: 0,
        words: [0; 20],
    };
    put(&mut t.words, 632, 8, u64::from(opcode));
    t
}
fn id<const N: usize>(words: &mut [u32; N], offset: usize, parts: &[u32]) {
    for (i, &value) in parts.iter().enumerate() {
        put(words, offset + i * 32, 32, u64::from(value));
    }
}
pub fn graph_ready(stage: StageId, epoch: Epoch) -> Transfer {
    let mut t = configuration(0);
    id(&mut t.words, 0, &stage.parts());
    put(&mut t.words, 96, 64, epoch.0);
    t
}
pub fn graph_release(stage: StageId) -> Transfer {
    let mut t = configuration(1);
    id(&mut t.words, 0, &stage.parts());
    t
}
pub fn pair_ready(stage: NodePairStageId, initial: Option<shoop_loops::LoopId>) -> Transfer {
    let mut t = configuration(2);
    id(&mut t.words, 0, &stage.parts());
    if let Some(target) = initial {
        put(&mut t.words, 96, 32, u64::from(target.slot()));
        put(&mut t.words, 128, 32, u64::from(target.generation()));
        put(&mut t.words, 160, 1, 1);
        put(&mut t.words, 161, 1, 1);
    }
    t
}
pub fn pair_initial(
    stage: NodePairStageId,
    initial: Option<shoop_loops::runtime::NodePairInitial>,
) -> Transfer {
    let mut t = pair_ready(stage, initial.map(|i| i.target));
    if let Some(initial) = initial {
        put(
            &mut t.words,
            161,
            1,
            u64::from(initial.audio_widths.map_or(2, |w| w[0]) == 2),
        );
    }
    t
}
pub fn pair_cancel(stage: NodePairStageId, recycle: bool) -> Transfer {
    let mut t = configuration(if recycle { 4 } else { 3 });
    id(&mut t.words, 0, &stage.parts());
    t
}
pub fn topology_ready(
    stage: staging::StageId,
    specification: [u32; 6],
    supported: bool,
) -> Transfer {
    let mut t = configuration(5);
    id(&mut t.words, 0, &stage.parts());
    t.words[2..8].copy_from_slice(&specification);
    put(&mut t.words, 256, 1, u64::from(supported));
    t
}
pub fn topology_cancel(stage: staging::StageId, recycle: bool) -> Transfer {
    let mut t = configuration(if recycle { 7 } else { 6 });
    id(&mut t.words, 0, &stage.parts());
    t
}
pub fn graph(stage: StageId, graph_epoch: Epoch, topology_epoch: Epoch, nonce: u64) -> Transfer {
    let mut t = Transfer {
        opcode: audio_actions::GRAPH,
        state: false,
        address: 0,
        words: [0; 20],
    };
    id(&mut t.words, 0, &stage.parts());
    put(&mut t.words, 96, 64, graph_epoch.0);
    put(&mut t.words, 160, 64, topology_epoch.0);
    put(&mut t.words, 224, 64, nonce);
    t
}
pub fn initial(
    graph: StageId,
    pairs: NodePairStageId,
    topology: staging::StageId,
    graph_epoch: Epoch,
    topology_epoch: Epoch,
    nonce: u64,
) -> Transfer {
    let mut t = Transfer {
        opcode: audio_actions::INITIAL,
        state: false,
        address: 0,
        words: [0; 20],
    };
    id(&mut t.words, 0, &graph.parts());
    id(&mut t.words, 96, &pairs.parts());
    id(&mut t.words, 192, &topology.parts());
    put(&mut t.words, 256, 64, graph_epoch.0);
    put(&mut t.words, 320, 64, topology_epoch.0);
    put(&mut t.words, 384, 64, nonce);
    t
}
