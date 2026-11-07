//! Prepared slot pools made of segments, and the runtime node identities of
//! every slot.
//!
//! Bus slot zero owns node slot zero and the bulk reader slot one. Each segment
//! then owns a contiguous node range: two nodes per strip (input and output
//! stage), followed by three per loop (playback, capture and fader). Appending a segment extends both the
//! slot indices and the node range without moving existing slots. Remaining bus
//! nodes follow the segments.

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::description::Segment;
use rt_runtime::NodeId;

#[derive(Clone, Copy, Debug)]
struct SegmentLayout {
    first_strip: usize,
    strips: usize,
    first_loop: usize,
    loops: usize,
    first_node: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct Pool {
    segments: Box<[SegmentLayout]>,
    strips: usize,
    loops: usize,
    nodes: usize,
    buses: usize,
    processors: usize,
}

fn node(slot: usize) -> NodeId {
    NodeId::from_parts(slot as u32, 1).expect("prepared node slot")
}

impl Pool {
    pub(crate) fn new(segments: &[Segment], buses: usize) -> Self {
        Self::with_processors(segments, buses, 0)
    }
    pub(crate) fn with_processors(segments: &[Segment], buses: usize, processors: usize) -> Self {
        let mut layouts = Vec::with_capacity(segments.len());
        let (mut strips, mut loops, mut nodes) = (0, 0, 2);
        for segment in segments {
            let layout = SegmentLayout {
                first_strip: strips,
                strips: segment.strips as usize,
                first_loop: loops,
                loops: segment.loops as usize,
                first_node: nodes,
            };
            strips += layout.strips;
            loops += layout.loops;
            nodes += 2 * layout.strips + 3 * layout.loops;
            layouts.push(layout);
        }
        Self {
            segments: layouts.into_boxed_slice(),
            strips,
            loops,
            nodes: nodes + 2 * buses - 1 + processors,
            processors,
            buses,
        }
    }
    pub(crate) fn strips(&self) -> usize {
        self.strips
    }
    pub(crate) fn loops(&self) -> usize {
        self.loops
    }
    pub(crate) fn buses(&self) -> usize {
        self.buses
    }
    pub(crate) fn nodes(&self) -> usize {
        self.nodes
    }
    pub(crate) fn bus(&self, slot: usize) -> NodeId {
        if slot == 0 {
            node(0)
        } else {
            node(self.nodes - self.processors - 2 * self.buses + slot)
        }
    }
    pub(crate) fn bus_input(&self, slot: usize) -> NodeId {
        node(self.nodes - self.processors - self.buses + slot)
    }
    pub(crate) fn processor(&self, slot: usize) -> NodeId {
        node(self.nodes - self.processors + slot)
    }
    pub(crate) fn processors(&self) -> usize {
        self.processors
    }
    pub(crate) fn reader(&self) -> NodeId {
        node(1)
    }
    fn strip_base(&self, strip: usize) -> usize {
        let segment = self
            .segments
            .iter()
            .find(|s| strip >= s.first_strip && strip < s.first_strip + s.strips)
            .expect("prepared strip slot");
        segment.first_node + 2 * (strip - segment.first_strip)
    }
    fn loop_base(&self, slot: usize) -> usize {
        let segment = self
            .segments
            .iter()
            .find(|s| slot >= s.first_loop && slot < s.first_loop + s.loops)
            .expect("prepared loop slot");
        segment.first_node + 2 * segment.strips + 3 * (slot - segment.first_loop)
    }
    pub(crate) fn strip_input(&self, strip: usize) -> NodeId {
        node(self.strip_base(strip))
    }
    pub(crate) fn strip_output(&self, strip: usize) -> NodeId {
        node(self.strip_base(strip) + 1)
    }
    pub(crate) fn playback(&self, slot: usize) -> NodeId {
        node(self.loop_base(slot))
    }
    pub(crate) fn capture(&self, slot: usize) -> NodeId {
        node(self.loop_base(slot) + 1)
    }
    pub(crate) fn fader(&self, slot: usize) -> NodeId {
        node(self.loop_base(slot) + 2)
    }
}
