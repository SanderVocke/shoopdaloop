#[derive(Clone, Copy, Debug)]
pub(crate) struct MidiCcSources<const N: usize> {
    sources: [Option<(u8, u8)>; N],
}

impl<const N: usize> Default for MidiCcSources<N> {
    fn default() -> Self {
        Self { sources: [None; N] }
    }
}

impl<const N: usize> MidiCcSources<N> {
    pub(crate) fn assign(&mut self, index: usize, channel: u8, controller: u8) -> bool {
        if index >= N || channel > 15 || controller > 127 {
            return false;
        }
        for source in &mut self.sources {
            if *source == Some((channel, controller)) {
                *source = None;
            }
        }
        self.sources[index] = Some((channel, controller));
        true
    }

    pub(crate) fn remove(&mut self, index: usize) {
        if let Some(source) = self.sources.get_mut(index) {
            *source = None;
        }
    }

    pub(crate) fn clear(&mut self) {
        self.sources.fill(None);
    }

    pub(crate) fn source(&self, index: usize) -> Option<(u8, u8)> {
        self.sources.get(index).copied().flatten()
    }

    pub(crate) fn matching_index(&self, channel: u8, controller: u8) -> Option<usize> {
        self.sources
            .iter()
            .position(|source| *source == Some((channel, controller)))
    }
}
