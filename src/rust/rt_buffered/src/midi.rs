use rt_runtime::buffer::MidiBuffer;

/// Borrowed controls for one synchronously executed quantum. Equal-time order is
/// deferred controls, ordinary input, then current global controls.
#[derive(Clone, Copy)]
pub struct MidiControls<'a> {
    pub deferred: &'a MidiBuffer,
    pub current: &'a MidiBuffer,
}

#[derive(Clone, Copy)]
pub struct MidiInput<'a> {
    buffers: [Option<&'a MidiBuffer>; 3],
}
impl<'a> MidiInput<'a> {
    pub fn new(ordinary: &'a MidiBuffer, controls: Option<MidiControls<'a>>) -> Self {
        Self {
            buffers: [
                controls.map(|controls| controls.deferred),
                (!(controls.is_some() && ordinary.is_recovery())).then_some(ordinary),
                controls.map(|controls| controls.current),
            ],
        }
    }
    pub fn len(self) -> usize {
        self.buffers
            .into_iter()
            .flatten()
            .map(MidiBuffer::len)
            .fold(0, usize::saturating_add)
    }
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }
    pub fn byte_len(self) -> usize {
        self.buffers
            .into_iter()
            .flatten()
            .map(MidiBuffer::byte_len)
            .fold(0, usize::saturating_add)
    }
    pub fn iter(self) -> MidiEvents<'a> {
        MidiEvents {
            input: self,
            positions: [0; 3],
        }
    }
}

pub struct MidiEvents<'a> {
    input: MidiInput<'a>,
    positions: [usize; 3],
}
impl<'a> Iterator for MidiEvents<'a> {
    type Item = (u32, &'a [u8]);
    fn next(&mut self) -> Option<Self::Item> {
        let (index, event) = self
            .input
            .buffers
            .iter()
            .enumerate()
            .filter_map(|(index, buffer)| {
                buffer
                    .and_then(|buffer| buffer.get(self.positions[index]))
                    .map(|event| (index, event))
            })
            .min_by_key(|(index, (offset, _))| (*offset, *index))?;
        self.positions[index] += 1;
        Some(event)
    }
}

impl<'a> From<&'a MidiBuffer> for MidiInput<'a> {
    fn from(ordinary: &'a MidiBuffer) -> Self {
        Self::new(ordinary, None)
    }
}
