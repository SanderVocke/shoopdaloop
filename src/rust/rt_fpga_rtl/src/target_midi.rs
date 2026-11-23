use alloc::{collections::BTreeMap, vec, vec::Vec};
use rt_fpga_compiler::{MidiOperation as M, Operation};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Lowering {
    Keep,
    Remove,
    Output { sources: [u8; 8], count: u8 },
}

#[derive(Default)]
struct List {
    producer: Option<usize>,
    uses: usize,
    ambiguous: bool,
}

#[derive(Default)]
struct FanIn {
    sources: [u8; 8],
    count: usize,
    merges: Vec<usize>,
}

impl FanIn {
    fn collect(
        &mut self,
        work: &[Operation],
        lists: &BTreeMap<u16, List>,
        list: u16,
        before: usize,
    ) -> Option<()> {
        let info = lists.get(&list)?;
        let producer = info.producer?;
        if info.ambiguous || producer >= before {
            return None;
        }
        if let Operation::MidiData(M::Merge { left, right, .. }) = work[producer] {
            if info.uses == 1 {
                if self.merges.len() == self.sources.len() - 1 {
                    return None;
                }
                self.merges.push(producer);
                self.collect(work, lists, left, producer)?;
                return self.collect(work, lists, right, producer);
            }
        }
        *self.sources.get_mut(self.count)? = u8::try_from(list).ok()?;
        self.count += 1;
        Some(())
    }
}

pub(super) fn merge_outputs(work: &[Operation]) -> Vec<Lowering> {
    let mut lists = BTreeMap::<u16, List>::new();
    for (index, op) in work.iter().enumerate() {
        let (destination, sources) = match *op {
            Operation::MidiData(M::Input { destination, .. } | M::Playback { destination, .. }) => {
                (Some(destination), [None, None])
            }
            Operation::MidiData(M::Merge {
                left,
                right,
                destination,
            }) => (Some(destination), [Some(left), Some(right)]),
            Operation::MidiData(M::Capture { source, .. } | M::Output { source, .. }) => {
                (None, [Some(source), None])
            }
            _ => continue,
        };
        for source in sources.into_iter().flatten() {
            lists.entry(source).or_default().uses += 1;
        }
        if let Some(destination) = destination {
            let list = lists.entry(destination).or_default();
            list.ambiguous |= list.producer.replace(index).is_some();
        }
    }
    let mut lowering = vec![Lowering::Keep; work.len()];
    for (index, op) in work.iter().enumerate() {
        if let Operation::MidiData(M::Output { source, .. }) = *op {
            let mut fan_in = FanIn::default();
            if fan_in.collect(work, &lists, source, index).is_some() && fan_in.count >= 2 {
                lowering[index] = Lowering::Output {
                    sources: fan_in.sources,
                    count: fan_in.count as u8,
                };
                for merge in fan_in.merges {
                    lowering[merge] = Lowering::Remove;
                }
            }
        }
    }
    lowering
}
