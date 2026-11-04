use super::*;
use crate::PreparedConfig;

pub(super) struct Layout {
    pub(super) channels: usize,
    pub(super) page_bytes: usize,
    pub(super) page_frames: usize,
    pub(super) ring_pages: usize,
    pub(super) max_frames: usize,
    pub(super) depth: usize,
    pub(super) node_limit: usize,
    pub(super) scratch_bytes: usize,
    pub(super) lease_limit: usize,
    pub(super) index_pages: usize,
    pub(super) transaction_pages: usize,
    pub(super) debt_limit: usize,
    pub(super) capacity: Capacity,
}

impl Config {
    /// Size a shared source pool without allocating or reserving arena pages.
    pub fn capacity(self, runtime: PreparedConfig, histories: usize) -> Result<Capacity, Error> {
        Layout::new(runtime, self, histories).map(|layout| layout.capacity)
    }
}

impl Layout {
    pub(super) fn new(
        runtime: PreparedConfig,
        config: Config,
        histories: usize,
    ) -> Result<Self, Error> {
        let channels = config.channels as usize;
        let page_bytes = runtime.limits().page_bytes;
        if channels == 0
            || config.history_frames == 0
            || config.views == 0
            || histories == 0
            || page_bytes / 4 < channels
        {
            return Err(Error::InvalidConfig);
        }
        let page_frames = page_bytes / 4 / channels;
        runtime
            .limits()
            .max_frames
            .checked_mul(channels)
            .and_then(|n| n.checked_mul(4))
            .filter(|&n| n <= i32::MAX as usize)
            .ok_or(Error::Capacity)?;
        let ring_pages = config
            .history_frames
            .div_ceil(page_frames)
            .checked_add(1)
            .and_then(usize::checked_next_power_of_two)
            .ok_or(Error::Capacity)?;
        let max_frames = runtime.limits().max_frames;
        let transaction_pages = ring_pages.min(
            max_frames
                .div_ceil(page_frames)
                .checked_add(2)
                .ok_or(Error::Capacity)?,
        );
        let history_pages = ring_pages.checked_mul(histories).ok_or(Error::Capacity)?;
        let baseline = history_pages
            .checked_add(transaction_pages)
            .ok_or(Error::Capacity)?;
        if config.pages < baseline || config.pages > runtime.limits().resource_pages {
            return Err(Error::Capacity);
        }
        let index_pages = (runtime.resource_bytes() / (channels * 4))
            .div_ceil(page_frames)
            .max(ring_pages)
            .checked_next_power_of_two()
            .ok_or(Error::Capacity)?;
        let depth = index_pages.trailing_zeros() as usize;
        let debt_limit = config.pages - baseline;
        let owner_limit = config.views.min(debt_limit);
        // Clipping leaves at most two intervals: at most two nodes per covered
        // page plus four boundary paths. Keep a displaced history and write scratch.
        let history_nodes = ring_pages
            .checked_mul(2)
            .and_then(|n| n.checked_add(depth.checked_mul(4)?))
            .and_then(|n| n.checked_mul(histories))
            .ok_or(Error::Capacity)?;
        let node_limit = history_nodes
            .checked_mul(2)
            .and_then(|n| n.checked_add(debt_limit.checked_mul(2)?))
            .and_then(|n| n.checked_add(owner_limit.checked_mul(depth.checked_mul(4)?)?))
            .and_then(|n| {
                n.checked_add(
                    transaction_pages
                        .checked_mul(depth + 1)?
                        .max(4 * (depth + 1)),
                )
            })
            .ok_or(Error::Capacity)?;
        let scratch_bytes = transaction_pages
            .checked_mul(page_bytes)
            .ok_or(Error::Capacity)?;
        let lease_limit = config
            .pages
            .checked_add(transaction_pages)
            .ok_or(Error::Capacity)?;
        let metadata_bytes = node_limit
            .checked_mul(size_of::<Option<Node>>() + size_of::<usize>() * 4)
            .and_then(|n| {
                n.checked_add(
                    config
                        .views
                        .checked_mul(size_of::<Slot<View>>() + size_of::<usize>() * 2)?,
                )
            })
            .and_then(|n| n.checked_add(index_pages.checked_mul(size_of::<(u64, usize)>())?))
            .and_then(|n| {
                n.checked_add(
                    config
                        .views
                        .checked_mul(size_of::<Option<Base>>() + size_of::<usize>())?,
                )
            })
            .and_then(|n| n.checked_add(scratch_bytes))
            .and_then(|n| n.checked_add(transaction_pages.checked_mul(size_of::<Plan>())?))
            .and_then(|n| {
                n.checked_add(histories.checked_mul(size_of::<History>() + size_of::<usize>())?)
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
                    lease_limit.checked_mul(size_of::<Slot<usize>>() + size_of::<usize>())?,
                )
            })
            .filter(|&n| n <= i32::MAX as usize)
            .ok_or(Error::Capacity)?;
        ring_pages
            .checked_mul(page_frames)
            .filter(|&n| n <= i32::MAX as usize)
            .ok_or(Error::Capacity)?;
        Ok(Self {
            channels,
            page_bytes,
            page_frames,
            ring_pages,
            max_frames,
            depth,
            node_limit,
            scratch_bytes,
            lease_limit,
            index_pages,
            transaction_pages,
            debt_limit,
            capacity: Capacity {
                histories,
                history_pages,
                transaction_pages,
                index_nodes: node_limit,
                metadata_bytes,
            },
        })
    }
}
