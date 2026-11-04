use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Counts {
    pub allocations: usize,
    pub reallocations: usize,
    pub deallocations: usize,
}

/// Requested allocation sizes in one isolated ownership scope, excluding allocator overhead.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Memory {
    pub allocated: usize,
    pub freed: usize,
    pub live: usize,
    pub peak: usize,
}
impl Memory {
    fn allocate(&mut self, bytes: usize) {
        self.allocated = self.allocated.saturating_add(bytes);
        self.live = self.live.saturating_add(bytes);
        self.peak = self.peak.max(self.live);
    }
    fn free(&mut self, bytes: usize) {
        self.freed = self.freed.saturating_add(bytes);
        self.live = self.live.saturating_sub(bytes);
    }
}

#[derive(Clone, Copy)]
struct State {
    enabled: bool,
    counts: Counts,
    memory: Memory,
}

thread_local! {
    static STATE: Cell<State> = const { Cell::new(State {
        enabled: false,
        counts: Counts { allocations: 0, reallocations: 0, deallocations: 0 },
        memory: Memory { allocated: 0, freed: 0, live: 0, peak: 0 },
    }) };
}

fn record(update: impl FnOnce(&mut Counts, &mut Memory)) {
    let _ = STATE.try_with(|state| {
        let mut current = state.get();
        if current.enabled {
            update(&mut current.counts, &mut current.memory);
            state.set(current);
        }
    });
}

pub struct TrackingAllocator;

// SAFETY: Every allocation operation forwards its original arguments to System.
// Bookkeeping uses const-initialized TLS with no allocation or unwinding; it does
// not inspect or change the returned memory or allocator ownership.
unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(|counts, memory| {
            counts.allocations = counts.allocations.saturating_add(1);
            memory.allocate(layout.size());
        });
        // SAFETY: The caller supplies the valid allocation layout required by System.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(|counts, memory| {
            counts.allocations = counts.allocations.saturating_add(1);
            memory.allocate(layout.size());
        });
        // SAFETY: The caller supplies the valid allocation layout required by System.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        record(|counts, memory| {
            counts.deallocations = counts.deallocations.saturating_add(1);
            memory.free(layout.size());
        });
        // SAFETY: The pointer and original layout are forwarded unchanged to System.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record(|counts, memory| {
            counts.reallocations = counts.reallocations.saturating_add(1);
            memory.free(layout.size());
            memory.allocate(size);
        });
        // SAFETY: The pointer, original layout and requested size come from the caller.
        unsafe { System.realloc(ptr, layout, size) }
    }
}

struct Scope;

impl Drop for Scope {
    fn drop(&mut self) {
        STATE.with(|state| {
            let mut current = state.get();
            current.enabled = false;
            state.set(current);
        });
    }
}

pub fn is_rendering() -> bool {
    STATE.with(|state| state.get().enabled)
}

pub fn measure<R>(run: impl FnOnce() -> R) -> (R, Counts) {
    assert!(!is_rendering(), "nested allocator scopes are not supported");
    STATE.with(|state| {
        state.set(State {
            enabled: true,
            counts: Counts::default(),
            memory: Memory::default(),
        });
    });
    let scope = Scope;
    let result = run();
    drop(scope);
    (result, STATE.with(|state| state.get().counts))
}

pub fn measure_memory<R>(run: impl FnOnce() -> R) -> (R, Memory) {
    let (result, _) = measure(run);
    (result, STATE.with(|state| state.get().memory))
}

pub fn assert_no_heap<R>(run: impl FnOnce() -> R) -> R {
    let (result, counts) = measure(run);
    assert_eq!(counts, Counts::default(), "heap operation during rendering");
    result
}

#[allow(dead_code)] // Shared support is compiled independently by each integration test.
pub struct DropProbe<'a> {
    pub realtime_drops: &'a Cell<usize>,
    pub control_drops: &'a Cell<usize>,
}

impl Drop for DropProbe<'_> {
    fn drop(&mut self) {
        let counter = if is_rendering() {
            self.realtime_drops
        } else {
            self.control_drops
        };
        counter.set(counter.get() + 1);
    }
}
