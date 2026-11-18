//! Transfers prepared images between a control endpoint and the exclusive RTL device owner.
use crate::ProgramLoader;
use anyhow::Result;
use rt_fpga_compiler::{loading::Layout, LogicalStateProgram, ProgramLimits};
use rt_fpga_rtl::{
    host::{control::descriptor, Upload},
    target::{ProgramImage, RtlTarget},
};
use shoop_rt_protocol::immediate::SwitchProgram;

pub struct Images {
    limits: ProgramLimits,
    uploads: rtrb::Producer<Upload>,
    ready: rtrb::Consumer<(u64, bool)>,
    retired: rtrb::Consumer<ProgramImage>,
}

pub struct ImageOwner {
    pub uploads: rtrb::Consumer<Upload>,
    pub ready: rtrb::Producer<(u64, bool)>,
    pub retired: rtrb::Producer<ProgramImage>,
}

pub fn image_channel(limits: ProgramLimits) -> (Images, ImageOwner) {
    let (uploads, upload_rx) = rtrb::RingBuffer::new(2);
    let (ready_tx, ready) = rtrb::RingBuffer::new(2);
    let (retire_tx, retired) = rtrb::RingBuffer::new(2);
    (
        Images {
            limits,
            uploads,
            ready,
            retired,
        },
        ImageOwner {
            uploads: upload_rx,
            ready: ready_tx,
            retired: retire_tx,
        },
    )
}

impl ProgramLoader for Images {
    fn limits(&self) -> ProgramLimits {
        self.limits
    }

    fn stage_program(
        &mut self,
        logical: &LogicalStateProgram,
        layout: Layout,
    ) -> Result<SwitchProgram> {
        let views = layout.media_views(logical);
        let image = RtlTarget {
            capacities: self.limits,
            media_views: &views,
            state_base: 0,
        }
        .build_image(logical, layout)?;
        let descriptor = descriptor(&image);
        self.uploads
            .push(Upload::new(image, self.limits.instructions))
            .map_err(|_| anyhow::anyhow!("RTL upload queue is full"))?;
        Ok(descriptor)
    }

    fn program_ready(&mut self) -> Option<(u64, bool)> {
        self.ready.pop().ok()
    }

    fn reclaim_programs(&mut self) {
        while self.retired.pop().is_ok() {}
    }
}
