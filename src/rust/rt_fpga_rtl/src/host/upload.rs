use crate::target::{ConfigurationWrite, ProgramImage};
use alloc::vec::Vec;

pub enum UploadStep<'a> {
    Configure(&'a ConfigurationWrite),
    ClearView(u32),
}

/// Prepare on the control task. Iterating the bounded set of steps needs no allocation.
pub struct Upload {
    pub image: ProgramImage,
    writes: Vec<ConfigurationWrite>,
    views: Vec<u32>,
    cursor: usize,
}

impl Upload {
    pub fn new(image: ProgramImage, region_depth: u32) -> Self {
        let writes = image.writes(u64::from(region_depth));
        let views = image.layout.new_media_views().collect();
        Self {
            image,
            writes,
            views,
            cursor: 0,
        }
    }

    pub fn next_step(&mut self) -> Option<UploadStep<'_>> {
        let index = self.cursor;
        if let Some(write) = self.writes.get(index) {
            self.cursor += 1;
            Some(UploadStep::Configure(write))
        } else {
            let view = *self.views.get(index - self.writes.len())?;
            self.cursor += 1;
            Some(UploadStep::ClearView(view))
        }
    }
}
