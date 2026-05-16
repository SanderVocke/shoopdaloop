//! CXX bridge for MIDI storage to expose to C++.

#![allow(dead_code)]

use crate::midi_storage::{
    FindResult, MidiCursor, MidiStorageCore, MidiStorageElem, MidiTimeWindow, TruncateSide,
};

// Sentinel value to represent "no value" / invalid offset
const INVALID_OFFSET: u32 = 0xFFFFFFFF;

#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    // Shared struct for MIDI storage elements (mirrored from midi_storage.rs)
    struct MidiStorageElem {
        time: u32,
        size: u16,
        bytes: [u8; 4],
    }

    extern "Rust" {
        // MidiStorageCore - the main storage type
        type MidiStorageCore;

        fn new_midi_storage_core(data_size_bytes: u32) -> Box<MidiStorageCore>;

        // State queries
        fn n_events(self: &MidiStorageCore) -> u32;
        fn capacity(self: &MidiStorageCore) -> u32;
        fn full(self: &MidiStorageCore) -> bool;
        fn empty(self: &MidiStorageCore) -> bool;

        // Raw ringbuffer access
        fn raw_tail(self: &MidiStorageCore) -> u32;
        fn raw_head(self: &MidiStorageCore) -> u32;
        fn raw_full(self: &MidiStorageCore) -> bool;

        // MidiCursor - cursor for iteration
        type MidiCursor;

        fn new_midi_cursor() -> Box<MidiCursor>;
        fn cursor_valid(cursor: &MidiCursor) -> bool;
        fn cursor_offset(cursor: &MidiCursor) -> u32;
        fn cursor_prev_offset(cursor: &MidiCursor) -> u32;
        fn cursor_invalidate(cursor: &mut MidiCursor);
        fn cursor_wrapped(cursor: &MidiCursor) -> bool;
        fn cursor_reset(cursor: &mut MidiCursor, storage: &MidiStorageCore);
        fn cursor_next(cursor: &mut MidiCursor, storage: &MidiStorageCore);
        fn cursor_overwrite(cursor: &mut MidiCursor, offset: u32, prev_offset: u32);
        fn cursor_find_time_forward(
            cursor: &mut MidiCursor,
            storage: &MidiStorageCore,
            time: u32,
        ) -> Box<FindResult>;

        // FindResult - search result type
        type FindResult;

        fn find_result_n_processed(res: &FindResult) -> u32;
        fn find_result_found_valid_elem(res: &FindResult) -> bool;

        // Append operation - returns true on success
        // dropped_cb_fn: raw function pointer (usize) to call for dropped messages (0 = no callback)
        // dropped_cb_ctx: user data passed to the callback function
        unsafe fn append(
            storage: &mut MidiStorageCore,
            time: u32,
            size: u16,
            data: *const u8,
            allow_replace: bool,
            dropped_cb_fn: usize,
            dropped_cb_ctx: usize,
        ) -> bool;

        // Truncate operation - removes messages and calls dropped_cb for each removed
        unsafe fn truncate(
            storage: &mut MidiStorageCore,
            time: u32,
            side: u8, // 0 = TruncateTail, 1 = TruncateHead
            dropped_cb_fn: usize,
            dropped_cb_ctx: usize,
        );

        fn clear_storage(storage: &mut MidiStorageCore);

        fn copy_to_storage(storage: &MidiStorageCore, target: &mut MidiStorageCore);
        fn copy_from_storage(storage: &mut MidiStorageCore, source: &MidiStorageCore);

        // MidiTimeWindow - time-window logic
        type MidiTimeWindow;

        fn new_midi_time_window() -> Box<MidiTimeWindow>;
        fn time_window_set_n_samples(window: &mut MidiTimeWindow, n: u32);
        fn time_window_get_n_samples(window: &MidiTimeWindow) -> u32;
        fn time_window_get_current_start_time(window: &MidiTimeWindow) -> u32;
        fn time_window_get_current_end_time(window: &MidiTimeWindow) -> u32;

        // next_buffer - advances time and truncates old messages
        // dropped_cb_fn/dropped_cb_ctx: callback for each dropped message
        unsafe fn time_window_next_buffer(
            window: &mut MidiTimeWindow,
            storage: &mut MidiStorageCore,
            n_frames: u32,
            dropped_cb_fn: usize,
            dropped_cb_ctx: usize,
        );

        // put operation - adds a message to the current buffer
        unsafe fn time_window_put(
            window: &mut MidiTimeWindow,
            storage: &mut MidiStorageCore,
            frame: u32,
            size: u16,
            data: *const u8,
            dropped_cb_fn: usize,
            dropped_cb_ctx: usize,
        ) -> bool;

        // snapshot operation - copies storage to target with adjusted timestamps
        unsafe fn time_window_snapshot(
            window: &MidiTimeWindow,
            storage: &mut MidiStorageCore,
            target: &mut MidiStorageCore,
            start_offset_from_end: u32,
        );

        // Data access for syncing C++ state
        // Physical offset access (raw array index)
        fn get_elem_time_at_physical_offset(storage: &MidiStorageCore, idx: u32) -> u32;
        fn get_elem_size_at_physical_offset(storage: &MidiStorageCore, idx: u32) -> u16;
        unsafe fn get_elem_bytes_at_physical_offset(
            storage: &MidiStorageCore,
            idx: u32,
            out: *mut u8,
            max_len: usize,
        );

        // Logical index access (0 = oldest, increasing toward newest)
        fn get_elem_time_at_logical_index(storage: &MidiStorageCore, idx: u32) -> u32;
        fn get_elem_size_at_logical_index(storage: &MidiStorageCore, idx: u32) -> u16;
        unsafe fn get_elem_bytes_at_logical_index(
            storage: &MidiStorageCore,
            idx: u32,
            out: *mut u8,
            max_len: usize,
        );

        // for_each_msg_modify - iterates and modifies all messages in storage
        // callback_fn: raw function pointer (usize) to call for each message
        // callback takes: (time: *mut u32, size: *mut u16, data: *mut u8, ctx: usize)
        unsafe fn for_each_msg_modify(
            storage: &mut MidiStorageCore,
            callback_fn: usize,
            callback_ctx: usize,
        );
    }
}

// Type for the callback function pointer
// Signature: void callback(uint32_t time, uint16_t size, const uint8_t* data, usize ctx)
type DroppedCbFn = unsafe extern "C" fn(u32, u16, *const u8, usize);

// Helper to call the dropped callback if provided
unsafe fn call_dropped_cb(cb_fn: usize, ctx: usize, time: u32, size: u16, data_ptr: *const u8) {
    if cb_fn != 0 {
        let fn_ptr: unsafe extern "C" fn(u32, u16, *const u8, usize) = std::mem::transmute(cb_fn);
        fn_ptr(time, size, data_ptr, ctx);
    }
}

/// Append operation with callback
unsafe fn append(
    storage: &mut MidiStorageCore,
    time: u32,
    size: u16,
    data: *const u8,
    allow_replace: bool,
    dropped_cb_fn: usize,
    dropped_cb_ctx: usize,
) -> bool {
    let slice = std::slice::from_raw_parts(data, size as usize);

    // Wrapper callback that calls the FFI callback
    let mut dropped_called = false;
    let cb_fn = dropped_cb_fn;
    let cb_ctx = dropped_cb_ctx;
    let mut wrapper_cb: Option<Box<dyn FnMut(u32, u16, *const u8, *mut std::ffi::c_void)>> =
        if cb_fn != 0 {
            Some(Box::new(
                move |t: u32, s: u16, d: *const u8, _ud: *mut std::ffi::c_void| {
                    if !dropped_called {
                        call_dropped_cb(cb_fn, cb_ctx, t, s, d);
                        dropped_called = true;
                    }
                },
            ))
        } else {
            None
        };

    storage.append(
        time,
        size,
        slice,
        allow_replace,
        wrapper_cb.as_mut(),
        dropped_cb_ctx as *mut std::ffi::c_void,
    )
}

/// Truncate operation with callback
unsafe fn truncate(
    storage: &mut MidiStorageCore,
    time: u32,
    side: u8,
    dropped_cb_fn: usize,
    dropped_cb_ctx: usize,
) {
    let side = if side == 0 {
        TruncateSide::TruncateTail
    } else {
        TruncateSide::TruncateHead
    };

    // Collect dropped messages first, then call callback
    let dropped_messages = if dropped_cb_fn != 0 {
        collect_dropped_messages(storage, time, side)
    } else {
        Vec::new()
    };

    // Actually truncate
    storage.truncate_no_callback(time, side);

    // Then call the callback for each dropped message
    for elem in dropped_messages {
        let data_ptr = elem.data().as_ptr();
        call_dropped_cb(
            dropped_cb_fn,
            dropped_cb_ctx,
            elem.time,
            elem.size,
            data_ptr,
        );
    }
}

// Helper to collect dropped messages before truncating
fn collect_dropped_messages(
    storage: &MidiStorageCore,
    time: u32,
    side: TruncateSide,
) -> Vec<MidiStorageElem> {
    let mut dropped = Vec::new();
    let capacity = storage.capacity();
    let n_events = storage.n_events();

    if capacity == 0 || n_events == 0 {
        return dropped;
    }

    match side {
        TruncateSide::TruncateHead => {
            let newest_idx = if storage.raw_head() == 0 {
                (capacity - 1) as u32
            } else {
                storage.raw_head() - 1
            };

            if let Some(elem) = storage.get_elem_at_physical_offset_ref(newest_idx) {
                if elem.time <= time {
                    return dropped;
                }
            }

            let mut idx = storage.raw_tail();
            for _ in 0..n_events {
                if let Some(elem) = storage.get_elem_at_physical_offset_ref(idx) {
                    if elem.time > time {
                        break;
                    }
                    dropped.push(elem.clone());
                }
                idx = (idx + 1) % capacity;
            }
        }
        TruncateSide::TruncateTail => {
            if let Some(elem) = storage.get_elem_at_physical_offset_ref(storage.raw_tail()) {
                if elem.time >= time {
                    return dropped;
                }
            }

            let mut idx = storage.raw_tail();
            for _ in 0..n_events {
                if let Some(elem) = storage.get_elem_at_physical_offset_ref(idx) {
                    if elem.time >= time {
                        break;
                    }
                    dropped.push(elem.clone());
                }
                idx = (idx + 1) % capacity;
            }
        }
    }

    dropped
}

// MidiStorageCore implementation
pub fn new_midi_storage_core(data_size_bytes: u32) -> Box<MidiStorageCore> {
    Box::new(MidiStorageCore::new(data_size_bytes))
}

// State queries
fn n_events(storage: &MidiStorageCore) -> u32 {
    storage.n_events()
}

fn capacity(storage: &MidiStorageCore) -> u32 {
    storage.capacity()
}

fn full(storage: &MidiStorageCore) -> bool {
    storage.full()
}

fn empty(storage: &MidiStorageCore) -> bool {
    storage.empty()
}

// Raw ringbuffer access
fn raw_tail(storage: &MidiStorageCore) -> u32 {
    storage.raw_tail()
}

fn raw_head(storage: &MidiStorageCore) -> u32 {
    storage.raw_head()
}

fn raw_full(storage: &MidiStorageCore) -> bool {
    storage.raw_full()
}

// FindResult accessors
fn find_result_n_processed(res: &FindResult) -> u32 {
    res.n_processed
}

fn find_result_found_valid_elem(res: &FindResult) -> bool {
    res.found_valid_elem
}

// MidiCursor implementation
pub fn new_midi_cursor() -> Box<MidiCursor> {
    Box::new(MidiCursor::new())
}

fn cursor_valid(cursor: &MidiCursor) -> bool {
    cursor.valid()
}

fn cursor_offset(cursor: &MidiCursor) -> u32 {
    cursor.offset().unwrap_or(INVALID_OFFSET)
}

fn cursor_prev_offset(cursor: &MidiCursor) -> u32 {
    cursor.prev_offset().unwrap_or(INVALID_OFFSET)
}

fn cursor_invalidate(cursor: &mut MidiCursor) {
    cursor.invalidate();
}

fn cursor_wrapped(cursor: &MidiCursor) -> bool {
    cursor.wrapped()
}

fn cursor_reset(cursor: &mut MidiCursor, storage: &MidiStorageCore) {
    cursor.reset(storage);
}

fn cursor_next(cursor: &mut MidiCursor, storage: &MidiStorageCore) {
    cursor.next(storage);
}

fn cursor_overwrite(cursor: &mut MidiCursor, offset: u32, prev_offset: u32) {
    cursor.overwrite(offset, prev_offset);
}

fn cursor_find_time_forward(
    cursor: &mut MidiCursor,
    storage: &MidiStorageCore,
    time: u32,
) -> Box<FindResult> {
    Box::new(cursor.find_time_forward(storage, time))
}

fn clear_storage(storage: &mut MidiStorageCore) {
    storage.clear();
}

fn copy_to_storage(storage: &MidiStorageCore, target: &mut MidiStorageCore) {
    storage.copy_to(target);
}

fn copy_from_storage(storage: &mut MidiStorageCore, source: &MidiStorageCore) {
    storage.copy_from(source);
}

// Physical offset data access
fn get_elem_time_at_physical_offset(storage: &MidiStorageCore, idx: u32) -> u32 {
    storage
        .get_elem_at_physical_offset_ref(idx)
        .map(|e| e.time)
        .unwrap_or(0)
}

fn get_elem_size_at_physical_offset(storage: &MidiStorageCore, idx: u32) -> u16 {
    storage
        .get_elem_at_physical_offset_ref(idx)
        .map(|e| e.size)
        .unwrap_or(0)
}

unsafe fn get_elem_bytes_at_physical_offset(
    storage: &MidiStorageCore,
    idx: u32,
    out: *mut u8,
    max_len: usize,
) {
    if let Some(elem) = storage.get_elem_at_physical_offset_ref(idx) {
        let len = std::cmp::min(elem.size as usize, max_len);
        std::ptr::copy_nonoverlapping(elem.data().as_ptr(), out, len);
    }
}

// Logical index data access
fn get_elem_time_at_logical_index(storage: &MidiStorageCore, idx: u32) -> u32 {
    storage
        .get_elem_logical_ref(idx)
        .map(|e| e.time)
        .unwrap_or(0)
}

fn get_elem_size_at_logical_index(storage: &MidiStorageCore, idx: u32) -> u16 {
    storage
        .get_elem_logical_ref(idx)
        .map(|e| e.size)
        .unwrap_or(0)
}

unsafe fn get_elem_bytes_at_logical_index(
    storage: &MidiStorageCore,
    idx: u32,
    out: *mut u8,
    max_len: usize,
) {
    if let Some(elem) = storage.get_elem_logical_ref(idx) {
        let len = std::cmp::min(elem.size as usize, max_len);
        std::ptr::copy_nonoverlapping(elem.data().as_ptr(), out, len);
    }
}

// Callback type for for_each_msg_modify
// Signature: fn(time: *mut u32, size: *mut u16, data: *mut u8, ctx: usize)
type ForEachModifyCallback = unsafe extern "C" fn(*mut u32, *mut u16, *mut u8, usize);

/// Iterate over all messages and apply a callback to modify them
/// The callback receives pointers to time, size, and data, allowing mutation
unsafe fn for_each_msg_modify(
    storage: &mut MidiStorageCore,
    callback_fn: usize,
    callback_ctx: usize,
) {
    if callback_fn == 0 {
        return;
    }
    let cb: ForEachModifyCallback = std::mem::transmute(callback_fn);

    let n_events = storage.n_events();
    let capacity = storage.capacity();
    if n_events == 0 || capacity == 0 {
        return;
    }

    let tail = storage.raw_tail();
    for i in 0..n_events {
        let phys_idx = (tail + i) % capacity;
        if let Some(elem) = storage.get_elem_at_physical_offset_mut(phys_idx) {
            let time_ptr = &mut elem.time as *mut u32;
            let size_ptr = &mut elem.size as *mut u16;
            let data_ptr = elem.bytes.as_mut_ptr();
            cb(time_ptr, size_ptr, data_ptr, callback_ctx);
        }
    }
}

// MidiTimeWindow implementation
pub fn new_midi_time_window() -> Box<MidiTimeWindow> {
    Box::new(MidiTimeWindow::new())
}

fn time_window_set_n_samples(window: &mut MidiTimeWindow, n: u32) {
    window.set_n_samples(n);
}

fn time_window_get_n_samples(window: &MidiTimeWindow) -> u32 {
    window.get_n_samples()
}

fn time_window_get_current_start_time(window: &MidiTimeWindow) -> u32 {
    window.get_current_start_time()
}

fn time_window_get_current_end_time(window: &MidiTimeWindow) -> u32 {
    window.get_current_end_time()
}

unsafe fn time_window_next_buffer(
    window: &mut MidiTimeWindow,
    storage: &mut MidiStorageCore,
    n_frames: u32,
    dropped_cb_fn: usize,
    dropped_cb_ctx: usize,
) {
    // Wrapper callback to capture dropped messages
    let mut dropped_cb: Option<Box<dyn FnMut(u32, u16, *const u8, *mut std::ffi::c_void)>> =
        if dropped_cb_fn != 0 {
            let cb_fn = dropped_cb_fn;
            let cb_ctx = dropped_cb_ctx;
            Some(Box::new(
                move |t: u32, s: u16, d: *const u8, _ud: *mut std::ffi::c_void| {
                    call_dropped_cb(cb_fn, cb_ctx, t, s, d);
                },
            ))
        } else {
            None
        };

    // Call next_buffer - the core handles overflow, truncation, and callback
    window.next_buffer(
        storage,
        n_frames,
        dropped_cb.as_mut(),
        dropped_cb_ctx as *mut std::ffi::c_void,
    );
}

unsafe fn time_window_put(
    window: &mut MidiTimeWindow,
    storage: &mut MidiStorageCore,
    frame: u32,
    size: u16,
    data: *const u8,
    dropped_cb_fn: usize,
    dropped_cb_ctx: usize,
) -> bool {
    let slice = std::slice::from_raw_parts(data, size as usize);

    // Wrapper callback
    let mut dropped_called = false;
    let cb_fn = dropped_cb_fn;
    let cb_ctx = dropped_cb_ctx;
    let mut wrapper_cb: Option<Box<dyn FnMut(u32, u16, *const u8, *mut std::ffi::c_void)>> =
        if dropped_cb_fn != 0 {
            Some(Box::new(
                move |t: u32, s: u16, d: *const u8, _ud: *mut std::ffi::c_void| {
                    if !dropped_called {
                        call_dropped_cb(cb_fn, cb_ctx, t, s, d);
                        dropped_called = true;
                    }
                },
            ))
        } else {
            None
        };

    window.put(
        storage,
        frame,
        size,
        slice,
        wrapper_cb.as_mut(),
        dropped_cb_ctx as *mut std::ffi::c_void,
    )
}

fn time_window_snapshot(
    window: &MidiTimeWindow,
    storage: &MidiStorageCore,
    target: &mut MidiStorageCore,
    start_offset_from_end: u32,
) {
    // MidiTimeWindow::snapshot expects (source, target) parameters
    // C++ calls it as: snapshot(m_storage, target) where m_storage is the ringbuffer source
    // So we swap the parameters: storage (ringbuffer source) goes to target parameter, and target (snapshot) goes to storage parameter
    window.snapshot(storage, target, start_offset_from_end);
}
