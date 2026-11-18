//! Production media/SP/JP model geometry, with an embedded AXI memory responder.
use crate::model::{Design, Probe};
#[derive(Clone, Debug)]
pub struct Geometry {
    pub views: u64,
    pub slots: u64,
    pub chunk_frames: u64,
    pub block_chunks: u64,
    pub table_pages: u64,
    pub retired_tables: u64,
    pub reclaim_blocks: u64,
    pub reclaim_cycles: u64,
    pub reclaim_guard: u64,
    pub table_lines: u64,
    pub reference_lines: u64,
    pub stack_lines: u64,
    pub read_buffers: u64,
    pub write_buffers: u64,
    pub reuse_views: bool,
}
impl Geometry {
    pub fn new(c: [u64; 3], block_chunks: u64) -> Self {
        assert!(block_chunks.is_power_of_two() && c[1] % block_chunks == 0);
        Self {
            views: c[0],
            slots: c[1],
            chunk_frames: c[2],
            block_chunks,
            table_pages: (c[1] / block_chunks).div_ceil(15) + 8 * c[0] + 16,
            retired_tables: 256,
            reclaim_blocks: 0,
            reclaim_cycles: 5000,
            reclaim_guard: 1024,
            table_lines: 8,
            reference_lines: 4,
            stack_lines: 2,
            read_buffers: 4,
            write_buffers: 4,
            reuse_views: false,
        }
    }
    pub fn blocks(&self) -> u64 {
        self.slots / self.block_chunks
    }
    pub fn slot_bits(&self) -> u32 {
        (64 - (self.blocks() - 1).leading_zeros()).max(1)
    }
    pub fn words(&self) -> usize {
        (self.slots * self.chunk_frames).div_ceil(16) as usize * 16
            + 2 * self.blocks().div_ceil(16) as usize * 16
            + self.table_pages as usize * 16
            + self.retired_tables as usize * 16
    }
    pub fn parameters(&self) -> Vec<(&'static str, u64)> {
        vec![
            ("VIEWS", self.views),
            ("SLOTS", self.slots),
            ("CHUNK_FRAMES", self.chunk_frames),
            ("BLOCK_CHUNKS", self.block_chunks),
            ("TABLE_PAGES", self.table_pages),
            ("RETIRED_TABLES", self.retired_tables),
            ("RECLAIM_BLOCKS", self.reclaim_blocks),
            ("RECLAIM_CYCLES", self.reclaim_cycles),
            ("RECLAIM_GUARD", self.reclaim_guard),
            ("TABLE_LINES", self.table_lines),
            ("REF_LINES", self.reference_lines),
            ("STACK_LINES", self.stack_lines),
            ("READ_BUFFERS", self.read_buffers),
            ("WRITE_BUFFERS", self.write_buffers),
            ("REUSE_VIEWS", u64::from(self.reuse_views)),
        ]
    }
}

pub fn design(g: &Geometry, limits: rt_fpga_compiler::ProgramLimits) -> Design {
    let root = crate::root();
    let mut sources = Vec::new();
    for (directory, names) in [
        (
            "state_processor",
            "sp_ram sp_job_fifo sp_engine state_processor",
        ),
        (
            "job_processor",
            "jp_sample_ram jp_divider job_processor audio_pipeline",
        ),
        ("chunk_memory", "chunk_ram chunk_priority chunk_memory"),
        (
            "media_store",
            "media_ram metadata_cache media_metadata media_reclaimer media_prefetch_resolver media_store memory_delay media_audio_pipeline",
        ),
        (
            "axi_memory",
            "axi_master axi_burst_mux shared_axi chunk_dma word_dma word_axi chunk_axi media_store_axi media_audio_pipeline_axi",
        ),
    ] {
        sources.extend(
            names
                .split_whitespace()
                .map(|name| root.join(format!("fpga/{directory}/rtl/{name}.sv"))),
        );
    }
    let mut parameters = g.parameters();
    parameters.extend([
        ("PROGRAM_DEPTH", limits.instructions.into()),
        ("STATE_DEPTH", limits.state_values.into()),
        ("JOB_DEPTH", limits.jobs.into()),
        ("MAX_FRAMES", limits.quantum.into()),
        ("BUFFERS", limits.buffers.into()),
        ("INPUT_PORTS", limits.input_ports.into()),
        ("OUTPUT_PORTS", limits.output_ports.into()),
        ("LIVE_LOADING", 1),
        ("PROGRAM_REGIONS", 2),
        ("AXI_DATA_WIDTH", 64),
        ("PHYSICAL_BASE", 0x01000000),
    ]);
    Design::new("media_audio_pipeline_axi", sources)
        .parameters(&parameters)
        .memory(g.words(), 0x01000000)
        .probes(
            [
                "core.unused_sync_event",
                "core.unused_sync_data",
                "core.sync_frame",
            ]
            .iter()
            .map(|p| Probe::observe(p))
            .collect(),
        )
}
