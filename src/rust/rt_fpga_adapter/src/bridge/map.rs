//! Generated host bridge register map; do not edit by hand.
pub const IDENTITY: u32 = 0x53484f50;
pub const VERSION_NUMBER: u32 = 1;
pub const ID: u32 = 0x000;
pub const VERSION: u32 = 0x004;
pub const CONTROL: u32 = 0x008;
pub const STATUS: u32 = 0x00c;
pub const QUANTA: u32 = 0x010;
pub const QUANTA_WORDS: usize = 2;
pub const LAST_CYCLES: u32 = 0x018;
pub const OUTPUT_WORDS: u32 = 0x01c;
pub const DIGEST: u32 = 0x020;
pub const DIGEST_WORDS: usize = 2;
pub const CAPACITY: u32 = 0x040;
pub const CAPACITY_WORDS: usize = 16;
pub const RUN: u32 = 0x080;
pub const RUN_WORDS: usize = 8;
pub const RUN_GO: u32 = 0x0a0;
pub const TP_OPCODE: u32 = 0x0c0;
pub const TP_INDEX: u32 = 0x0c4;
pub const TP_GO: u32 = 0x0c8;
pub const TP_ERROR: u32 = 0x0cc;
pub const TP_DATA: u32 = 0x100;
pub const TP_DATA_WORDS: usize = 20;
pub const TP_RESPONSE: u32 = 0x180;
pub const TP_RESPONSE_WORDS: usize = 26;
pub const CFG_CONTROL: u32 = 0x200;
pub const CFG_ADDRESS: u32 = 0x204;
pub const CFG_GO: u32 = 0x208;
pub const CFG_ERROR: u32 = 0x20c;
pub const CFG_WRITE: u32 = 0x210;
pub const CFG_WRITE_WORDS: usize = 5;
pub const CFG_READ: u32 = 0x230;
pub const CFG_READ_WORDS: usize = 5;
pub const AUDIO_CONTROL: u32 = 0x280;
pub const AUDIO_ADDRESS: u32 = 0x284;
pub const AUDIO_WRITE: u32 = 0x288;
pub const AUDIO_GO: u32 = 0x28c;
pub const AUDIO_READ: u32 = 0x290;
pub const AUDIO_ERROR: u32 = 0x294;
pub const STORE_CFG_CONTROL: u32 = 0x300;
pub const STORE_CFG_ADDRESS: u32 = 0x304;
pub const STORE_CFG_GO: u32 = 0x308;
pub const STORE_CFG_ERROR: u32 = 0x30c;
pub const STORE_CFG_WRITE: u32 = 0x310;
pub const STORE_CFG_WRITE_WORDS: usize = 5;
pub const STORE_CFG_READ: u32 = 0x330;
pub const STORE_CFG_READ_WORDS: usize = 5;
pub const STORE_OPCODE: u32 = 0x380;
pub const STORE_VIEW: u32 = 0x384;
pub const STORE_START: u32 = 0x388;
pub const STORE_START_WORDS: usize = 2;
pub const STORE_LENGTH: u32 = 0x390;
pub const STORE_LENGTH_WORDS: usize = 2;
pub const STORE_COUNT: u32 = 0x398;
pub const STORE_GO: u32 = 0x39c;
pub const STORE_RESULT: u32 = 0x3a0;
pub const STORE_RESULT_VIEW: u32 = 0x3a4;
pub const STORE_TX: u32 = 0x3a8;
pub const STORE_TX_SPACE: u32 = 0x3ac;
pub const STORE_RX: u32 = 0x3b0;
pub const STORE_RX_LEVEL: u32 = 0x3b4;
pub const STORE_WRITTEN: u32 = 0x3b8;
pub const STATUS_FAULTED: (u32, u32) = (0, 1);
pub const STATUS_ERROR_STAGE: (u32, u32) = (1, 2);
pub const STATUS_ERROR_CODE: (u32, u32) = (8, 8);
pub const STATUS_RUN_BUSY: (u32, u32) = (16, 1);
pub const STATUS_TP_BUSY: (u32, u32) = (17, 1);
pub const STATUS_CFG_BUSY: (u32, u32) = (18, 1);
pub const STATUS_AUDIO_BUSY: (u32, u32) = (19, 1);
pub const STATUS_STORE_CFG_BUSY: (u32, u32) = (20, 1);
pub const STATUS_STORE_BUSY: (u32, u32) = (21, 1);
pub const STATUS_RESET_BUSY: (u32, u32) = (22, 1);
pub const STATUS_QUIESCENT: (u32, u32) = (23, 1);
pub const STATUS_DONE: (u32, u32) = (24, 1);
pub const STATUS_MEMORY_FAULT: (u32, u32) = (25, 1);
pub const CAPACITIES: [&str; 16] = [
    "PROGRAM_DEPTH",
    "STATE_DEPTH",
    "BUFFERS",
    "MAX_FRAMES",
    "INPUT_PORTS",
    "OUTPUT_PORTS",
    "LOOP_DEPTH",
    "REQUEST_DEPTH",
    "EVENT_DEPTH",
    "VIEWS",
    "SLOTS",
    "BLOCK_CHUNKS",
    "CHUNK_FRAMES",
    "EXTERNAL_IO",
    "AXI_DATA_WIDTH",
    "PHYSICAL_BASE",
];
