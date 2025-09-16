use std::sync::atomic::AtomicU32;

struct HostSharedState {
    n_last_processed: AtomicU32,
}
