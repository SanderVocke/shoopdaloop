//! External-memory model. Chunk tables and payloads are accessible only to the job side.

use alloc::{vec, vec::Vec};

use crate::{fixed::Fixed, Error};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChunkId(pub usize);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViewId(pub usize);

struct View {
    chunks: Vec<ChunkId>,
    frames: usize,
    sealed: bool,
}

/// Mono chunks; represent additional channels using separate views/graph paths.
/// IDs never recycle in this first model, and sealed payloads are immutable.
pub struct MediaStore {
    chunk_frames: usize,
    chunks: Vec<Vec<Fixed>>,
    views: Vec<View>,
}

impl MediaStore {
    pub fn new(chunk_frames: usize) -> Result<Self, Error> {
        if chunk_frames == 0 {
            return Err(Error::InvalidShape);
        }
        Ok(Self {
            chunk_frames,
            chunks: Vec::new(),
            views: Vec::new(),
        })
    }

    pub fn create_recording(&mut self) -> ViewId {
        let id = ViewId(self.views.len());
        self.views.push(View {
            chunks: Vec::new(),
            frames: 0,
            sealed: false,
        });
        id
    }

    pub fn insert(&mut self, samples: &[Fixed]) -> Result<ViewId, Error> {
        let id = self.create_recording();
        self.append(id, samples)?;
        self.seal(id)?;
        Ok(id)
    }

    pub fn frames(&self, id: ViewId) -> Result<usize, Error> {
        Ok(self.views.get(id.0).ok_or(Error::InvalidView)?.frames)
    }

    pub fn seal(&mut self, id: ViewId) -> Result<(), Error> {
        self.views.get_mut(id.0).ok_or(Error::InvalidView)?.sealed = true;
        Ok(())
    }

    pub fn append(&mut self, id: ViewId, samples: &[Fixed]) -> Result<(), Error> {
        let view = self.views.get_mut(id.0).ok_or(Error::InvalidView)?;
        if view.sealed {
            return Err(Error::InvalidView);
        }
        for &sample in samples {
            if view.frames % self.chunk_frames == 0 {
                view.chunks.push(ChunkId(self.chunks.len()));
                self.chunks.push(vec![Fixed::ZERO; self.chunk_frames]);
            }
            let chunk = view.chunks[view.frames / self.chunk_frames];
            self.chunks[chunk.0][view.frames % self.chunk_frames] = sample;
            view.frames += 1;
        }
        Ok(())
    }

    pub fn read_loop(
        &self,
        id: ViewId,
        position: u64,
        length: u64,
        output: &mut [Fixed],
    ) -> Result<(), Error> {
        let view = self.views.get(id.0).ok_or(Error::InvalidView)?;
        if !view.sealed || length == 0 || length > view.frames as u64 || position >= length {
            return Err(Error::InvalidView);
        }
        let mut frame = position;
        for sample in output {
            let at = frame as usize;
            let chunk = view.chunks[at / self.chunk_frames];
            *sample = self.chunks[chunk.0][at % self.chunk_frames];
            frame = if frame + 1 == length { 0 } else { frame + 1 };
        }
        Ok(())
    }
}

impl crate::jobs::Memory for MediaStore {
    fn seal(&mut self, view: ViewId) -> Result<(), Error> {
        MediaStore::seal(self, view)
    }
    fn read(
        &mut self,
        view: ViewId,
        position: u64,
        length: u64,
        output: &mut [Fixed],
    ) -> Result<(), Error> {
        self.read_loop(view, position, length, output)
    }
    fn write(&mut self, view: ViewId, samples: &[Fixed]) -> Result<(), Error> {
        self.append(view, samples)
    }
}
