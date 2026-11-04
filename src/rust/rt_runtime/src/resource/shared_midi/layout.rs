use super::*;
use crate::PreparedConfig;

pub(super) struct Layout {
    pub(super) block_pages: usize,
    pub(super) depth: usize,
    pub(super) chunks: usize,
    pub(super) nodes: usize,
    pub(super) blocks: usize,
    pub(super) base_slots: usize,
    pub(super) map: usize,
    pub(super) garbage: usize,
    pub(super) debt_limit: usize,
    pub(super) capacity: Capacity,
}

impl Config {
    /// Validate and size all prepared arrays without allocating or reserving pages.
    pub fn capacity(self, runtime: PreparedConfig) -> Result<Capacity, Error> {
        Layout::new(runtime, self).map(|layout| layout.capacity)
    }
}

impl Layout {
    pub(super) fn new(runtime: PreparedConfig, config: Config) -> Result<Self, Error> {
        let page_bytes = runtime.limits().page_bytes;
        if config.history_frames == 0
            || config.chunks == 0
            || !config.chunks.is_power_of_two()
            || config.views == 0
            || config.transactions == 0
            || config.edit_steps == 0
            || config.max_events == 0
            || config.max_events > runtime.limits().midi_events_per_port
            || config.max_bytes > runtime.limits().midi_bytes_per_port
            || config.max_bytes == 0
            || config.max_packet == 0
            || config.max_packet > config.max_bytes
            || config.max_packet > runtime.limits().midi_bytes_per_port
        {
            return Err(Error::InvalidConfig);
        }
        let block_pages = config.max_packet.div_ceil(page_bytes).max(1);
        let credit = config
            .chunks
            .checked_mul(EVENTS_PER_CHUNK)
            .and_then(|n| n.checked_mul(block_pages))
            .ok_or(Error::Capacity)?;
        let workspace = config
            .max_bytes
            .div_ceil(page_bytes)
            .checked_add(config.max_events)
            .and_then(|n| n.checked_add(1))
            .ok_or(Error::Capacity)?;
        let baseline = credit.checked_add(workspace).ok_or(Error::Capacity)?;
        if config.pages < baseline || config.pages > runtime.limits().resource_pages {
            return Err(Error::Capacity);
        }
        let depth = config.chunks.trailing_zeros() as usize;
        let owners = config.views.min((config.pages - baseline) / credit);
        let chunks = owners
            .checked_add(config.transactions)
            .and_then(|n| n.checked_add(4))
            .and_then(|n| n.checked_mul(config.chunks))
            .and_then(|n| n.checked_add(config.max_events + 1))
            .ok_or(Error::Capacity)?;
        let nodes = owners
            .checked_add(config.transactions)
            .and_then(|n| n.checked_add(4))
            .and_then(|n| n.checked_mul(config.chunks.checked_mul(2)?.checked_sub(1)?))
            .and_then(|n| n.checked_add(config.max_events.checked_add(1)?.checked_mul(depth + 1)?))
            .ok_or(Error::Capacity)?;
        let blocks = config
            .pages
            .checked_add(
                chunks
                    .checked_mul(EVENTS_PER_CHUNK)
                    .ok_or(Error::Capacity)?,
            )
            .ok_or(Error::Capacity)?;
        let base_slots = config
            .views
            .checked_add(config.transactions)
            .ok_or(Error::Capacity)?;
        let map = blocks.checked_mul(block_pages).ok_or(Error::Capacity)?;
        let garbage = nodes
            .checked_add(chunks)
            .and_then(|n| n.checked_add(blocks))
            .and_then(|n| n.checked_add(config.transactions))
            .ok_or(Error::Capacity)?;
        let state_bytes = chunks
            .checked_add(config.views)
            .and_then(|n| n.checked_add(config.transactions.checked_mul(3)?))
            .and_then(|n| n.checked_add(1))
            .and_then(|n| n.checked_mul(size_of::<MidiState>()))
            .ok_or(Error::Capacity)?;
        let metadata_bytes = nodes
            .checked_mul(size_of::<Option<Node>>() + size_of::<usize>())
            .and_then(|n| {
                n.checked_add(chunks.checked_mul(size_of::<Option<Chunk>>() + size_of::<usize>())?)
            })
            .and_then(|n| {
                n.checked_add(blocks.checked_mul(size_of::<Option<Block>>() + size_of::<usize>())?)
            })
            .and_then(|n| n.checked_add(map.checked_mul(size_of::<Option<LeaseId>>())?))
            .and_then(|n| {
                n.checked_add(
                    config
                        .views
                        .checked_mul(size_of::<Slot<View>>() + 2 * size_of::<usize>())?,
                )
            })
            .and_then(|n| n.checked_add(garbage.checked_mul(2 * size_of::<Job>())?))
            .and_then(|n| {
                n.checked_add(
                    base_slots.checked_mul(size_of::<Option<Base>>() + size_of::<usize>())?,
                )
            })
            .and_then(|n| n.checked_add(state_bytes))
            .and_then(|n| n.checked_add(page_bytes))
            .and_then(|n| {
                n.checked_add(
                    config
                        .transactions
                        .checked_mul(size_of::<Slot<edit::Edit>>() + 2 * size_of::<usize>())?,
                )
            })
            .and_then(|n| {
                n.checked_add(
                    config
                        .transactions
                        .checked_mul(config.max_events)?
                        .checked_mul(size_of::<Record>())?,
                )
            })
            .and_then(|n| {
                n.checked_add(
                    runtime
                        .limits()
                        .resource_pages
                        .checked_mul(size_of::<usize>())?,
                )
            })
            .and_then(|n| {
                n.checked_add(
                    config
                        .pages
                        .checked_mul(size_of::<Slot<usize>>() + size_of::<usize>())?,
                )
            })
            .and_then(|n| n.checked_add(2 * size_of::<usize>() + size_of::<MidiStore>()))
            .filter(|&n| n <= i32::MAX as usize)
            .ok_or(Error::Capacity)?;
        Ok(Self {
            block_pages,
            depth,
            chunks,
            nodes,
            blocks,
            base_slots,
            map,
            garbage,
            debt_limit: (config.pages - baseline) / credit,
            capacity: Capacity {
                owners,
                chunks,
                index_nodes: nodes,
                state_bytes,
                metadata_bytes,
            },
        })
    }
}
