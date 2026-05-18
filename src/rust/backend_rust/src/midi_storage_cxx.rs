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
        fn cursor_is_at_start(cursor: &MidiCursor, storage: &MidiStorageCore) -> bool;
        fn cursor_next(cursor: &mut MidiCursor, storage: &MidiStorageCore);
        fn cursor_overwrite(cursor: &mut MidiCursor, offset: u32, prev_offset: u32);
        fn cursor_find_time_forward(
            cursor: &mut MidiCursor,
            storage: &MidiStorageCore,
            time: u32,
        ) -> Box<FindResult>;

        // find_fn_forward - find first element matching predicate
        // predicate_fn: raw function pointer (usize) to call for each message
        //   callback takes: (time: u32, size: u16, data: *const u8) -> bool (true = match/found)
        unsafe fn cursor_find_fn_forward(
            cursor: &mut MidiCursor,
            storage: &MidiStorageCore,
            predicate_fn: usize,
            predicate_ctx: usize,
        ) -> Box<FindResult>;

        // cursor_find_fn_forward_with_skip - variant that also calls a skip callback
        // skip_cb_fn: raw function pointer to call for each non-matching element
        //   callback takes: (time: u32, size: u16, data: *const u8, ctx: usize)
        //   returns: nothing (void)
        unsafe fn cursor_find_fn_forward_with_skip(
            cursor: &mut MidiCursor,
            storage: &MidiStorageCore,
            predicate_fn: usize,
            predicate_ctx: usize,
            skip_cb_fn: usize,
            skip_cb_ctx: usize,
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

        unsafe fn time_window_next_buffer(
            window: &mut MidiTimeWindow,
            storage: &mut MidiStorageCore,
            n_frames: u32,
            dropped_cb_fn: usize,
            dropped_cb_ctx: usize,
        );

        // put - add a message at a specific frame
        // dropped_cb_fn: raw function pointer (usize) to call for dropped messages (0 = no callback)
        // dropped_cb_ctx: user data passed to the callback function
        unsafe fn time_window_put(
            window: &mut MidiTimeWindow,
            storage: &mut MidiStorageCore,
            frame_in_current_buffer: u32,
            size: u16,
            data: *const u8,
            dropped_cb_fn: usize,
            dropped_cb_ctx: usize,
        ) -> bool;

        // snapshot - copy storage with time adjustment
        unsafe fn time_window_snapshot(
            window: &MidiTimeWindow,
            storage: &MidiStorageCore,
            target: &mut MidiStorageCore,
            start_offset_from_end: u32,
        );

        // Physical offset access
        unsafe fn get_elem_at_physical_offset(
            storage: &MidiStorageCore,
            idx: u32,
            out_time: &mut u32,
            out_size: &mut u16,
            out_bytes: &mut [u8; 4],
        ) -> bool;

        // Logical index access
        unsafe fn get_elem_at_logical_index(
            storage: &MidiStorageCore,
            idx: u32,
            out_time: &mut u32,
            out_size: &mut u16,
            out_bytes: &mut [u8; 4],
        ) -> bool;

        // Iterate over all messages, calling a callback for each
        // The callback signature: fn(time: *mut u32, size: *mut u16, data: *mut u8, ctx: usize)
        unsafe fn for_each_msg_modify(
            storage: &mut MidiStorageCore,
            callback_fn: usize,
            callback_ctx: usize,
        );

        // prepend - adds a message at the tail (earlier in time)
        // dropped_cb_fn/dropped_cb_ctx: callback for dropped messages when buffer is full (0 = no callback)
        unsafe fn prepend(
            storage: &mut MidiStorageCore,
            time: u32,
            size: u16,
            data: *const u8,
            dropped_cb_fn: usize,
            dropped_cb_ctx: usize,
        ) -> bool;

        // truncate_fn - truncate with a predicate function
        // predicate_fn: raw function pointer to call for each message
        //   returns true if the message should be dropped
        //   callback takes: (time: u32, size: u16, data: *const u8, ctx: usize) -> bool
        // dropped_cb_fn/dropped_cb_ctx: callback for each dropped message (0 = no callback)
        unsafe fn truncate_fn(
            storage: &mut MidiStorageCore,
            predicate_fn: usize,
            predicate_ctx: usize,
            side: u8, // 0 = TruncateTail, 1 = TruncateHead
            dropped_cb_fn: usize,
            dropped_cb_ctx: usize,
        );
    }
}

// CXX FFI function implementations

fn new_midi_storage_core(data_size_bytes: u32) -> Box<MidiStorageCore> {
    Box::new(MidiStorageCore::new(data_size_bytes))
}

fn new_midi_cursor() -> Box<MidiCursor> {
    Box::new(MidiCursor::new())
}

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

fn raw_tail(storage: &MidiStorageCore) -> u32 {
    storage.raw_tail()
}

fn raw_head(storage: &MidiStorageCore) -> u32 {
    storage.raw_head()
}

fn raw_full(storage: &MidiStorageCore) -> bool {
    storage.raw_full()
}

// Cursor FFI functions
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
    cursor.invalidate()
}

fn cursor_wrapped(cursor: &MidiCursor) -> bool {
    cursor.wrapped()
}

fn cursor_reset(cursor: &mut MidiCursor, storage: &MidiStorageCore) {
    cursor.reset(storage)
}

fn cursor_is_at_start(cursor: &MidiCursor, storage: &MidiStorageCore) -> bool {
    cursor.is_at_start(storage)
}

fn cursor_next(cursor: &mut MidiCursor, storage: &MidiStorageCore) {
    cursor.next(storage)
}

fn cursor_overwrite(cursor: &mut MidiCursor, offset: u32, prev_offset: u32) {
    cursor.overwrite(offset, prev_offset)
}

fn cursor_find_time_forward(
    cursor: &mut MidiCursor,
    storage: &MidiStorageCore,
    time: u32,
) -> Box<FindResult> {
    Box::new(cursor.find_time_forward(storage, time))
}

// Predicate callback type for cursor_find_fn_forward
// Signature: bool predicate(uint32_t time, uint16_t size, const uint8_t* data, usize ctx)
type CursorPredicateFn = unsafe extern "C" fn(u32, u16, *const u8, usize) -> bool;

/// Find first element matching predicate function
/// predicate_fn: raw function pointer to call for each message
///   returns true if the element matches (found, stop searching)
///   callback takes: (time: u32, size: u16, data: *const u8, ctx: usize) -> bool
unsafe fn cursor_find_fn_forward(
    cursor: &mut MidiCursor,
    storage: &MidiStorageCore,
    predicate_fn: usize,
    predicate_ctx: usize,
) -> Box<FindResult> {
    if predicate_fn == 0 {
        return Box::new(crate::midi_storage::FindResult {
            n_processed: 0,
            found_valid_elem: false,
        });
    }

    let pred_fn = std::mem::transmute::<usize, CursorPredicateFn>(predicate_fn);

    let pred = |elem: &MidiStorageElem| -> bool {
        pred_fn(elem.time, elem.size, elem.data().as_ptr(), predicate_ctx)
    };

    Box::new(cursor.find_fn_forward(storage, pred))
}

// Skip callback type for cursor_find_fn_forward_with_skip
// Signature: void skip_callback(uint32_t time, uint16_t size, const uint8_t* data, usize ctx)
type CursorSkipCallbackFn = unsafe extern "C" fn(u32, u16, *const u8, usize);

/// Find first element matching predicate function, with skip callback
/// For each element that doesn't match the predicate, calls skip_cb_fn
/// predicate_fn: raw function pointer - returns true if element matches
/// skip_cb_fn: raw function pointer - called for each non-matching element
/// skip_cb_ctx: user data passed to the skip callback
unsafe fn cursor_find_fn_forward_with_skip(
    cursor: &mut MidiCursor,
    storage: &MidiStorageCore,
    predicate_fn: usize,
    predicate_ctx: usize,
    skip_cb_fn: usize,
    skip_cb_ctx: usize,
) -> Box<FindResult> {
    if predicate_fn == 0 {
        return Box::new(crate::midi_storage::FindResult {
            n_processed: 0,
            found_valid_elem: false,
        });
    }

    let pred_fn = std::mem::transmute::<usize, CursorPredicateFn>(predicate_fn);

    let mut skip_callback: Option<CursorSkipCallbackFn> = None;
    if skip_cb_fn != 0 {
        skip_callback = Some(std::mem::transmute::<usize, CursorSkipCallbackFn>(
            skip_cb_fn,
        ));
    }

    let pred = |elem: &MidiStorageElem| -> bool {
        pred_fn(elem.time, elem.size, elem.data().as_ptr(), predicate_ctx)
    };

    Box::new(cursor.find_fn_forward_with_skip(storage, pred, |elem| {
        if let Some(ref cb) = skip_callback {
            cb(elem.time, elem.size, elem.data().as_ptr(), skip_cb_ctx);
        }
    }))
}

fn find_result_n_processed(res: &FindResult) -> u32 {
    res.n_processed
}

fn find_result_found_valid_elem(res: &FindResult) -> bool {
    res.found_valid_elem
}

// Append dropped callback type
// Signature: void callback(uint32_t time, uint16_t size, const uint8_t* data, usize ctx)
type AppendDroppedCb = unsafe extern "C" fn(u32, u16, *const u8, usize);

/// Append a message to storage
unsafe fn append(
    storage: &mut MidiStorageCore,
    time: u32,
    size: u16,
    data: *const u8,
    allow_replace: bool,
    dropped_cb_fn: usize,
    dropped_cb_ctx: usize,
) -> bool {
    let data_slice = std::slice::from_raw_parts(data, size as usize);

    let mut dropped_cb_wrapper: Option<Box<dyn FnMut(u32, u16, *const u8, *mut std::ffi::c_void)>> =
        None;
    if dropped_cb_fn != 0 {
        let cb_fn = dropped_cb_fn;
        let cb_ctx = dropped_cb_ctx;
        dropped_cb_wrapper = Some(Box::new(move |t, s, d, _| {
            let cb: AppendDroppedCb = std::mem::transmute(cb_fn);
            cb(t, s, d, cb_ctx);
        }));
    }

    storage.append(
        time,
        size,
        data_slice,
        allow_replace,
        dropped_cb_wrapper.as_mut(),
        std::ptr::null_mut() as *mut std::ffi::c_void,
    )
}

// Truncate dropped callback type
// Signature: void callback(uint32_t time, uint16_t size, const uint8_t* data, usize ctx)
type TruncateDroppedCb = unsafe extern "C" fn(u32, u16, *const u8, usize);

/// Truncate storage
unsafe fn truncate(
    storage: &mut MidiStorageCore,
    time: u32,
    side: u8,
    dropped_cb_fn: usize,
    dropped_cb_ctx: usize,
) {
    let truncate_side = if side == 0 {
        TruncateSide::TruncateTail
    } else {
        TruncateSide::TruncateHead
    };

    let mut dropped_cb_wrapper: Option<Box<dyn FnMut(u32, u16, *const u8, *mut std::ffi::c_void)>> =
        None;
    if dropped_cb_fn != 0 {
        let cb_fn = dropped_cb_fn;
        let cb_ctx = dropped_cb_ctx;
        dropped_cb_wrapper = Some(Box::new(move |t, s, d, _| {
            let cb: TruncateDroppedCb = std::mem::transmute(cb_fn);
            cb(t, s, d, cb_ctx);
        }));
    }

    storage.truncate(
        time,
        truncate_side,
        dropped_cb_wrapper.as_mut(),
        std::ptr::null_mut() as *mut std::ffi::c_void,
    )
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

/// Get element at physical offset - returns all fields via output parameters
/// Returns true if element exists, false otherwise
unsafe fn get_elem_at_physical_offset(
    storage: &MidiStorageCore,
    idx: u32,
    out_time: &mut u32,
    out_size: &mut u16,
    out_bytes: &mut [u8; 4],
) -> bool {
    if let Some(elem) = storage.get_elem_at_physical_offset_ref(idx) {
        *out_time = elem.time;
        *out_size = elem.size;
        out_bytes.copy_from_slice(&elem.bytes);
        true
    } else {
        false
    }
}

/// Get element at logical index (0 = oldest) - returns all fields via output parameters
/// Returns true if element exists, false otherwise
unsafe fn get_elem_at_logical_index(
    storage: &MidiStorageCore,
    idx: u32,
    out_time: &mut u32,
    out_size: &mut u16,
    out_bytes: &mut [u8; 4],
) -> bool {
    if let Some(elem) = storage.get_elem_logical_ref(idx) {
        *out_time = elem.time;
        *out_size = elem.size;
        out_bytes.copy_from_slice(&elem.bytes);
        true
    } else {
        false
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
    let mut dropped_cb_wrapper: Option<Box<dyn FnMut(u32, u16, *const u8, *mut std::ffi::c_void)>> =
        None;
    if dropped_cb_fn != 0 {
        let cb_fn = dropped_cb_fn;
        let cb_ctx = dropped_cb_ctx;
        dropped_cb_wrapper = Some(Box::new(move |t, s, d, _| {
            let cb: TruncateDroppedCb = std::mem::transmute(cb_fn);
            cb(t, s, d, cb_ctx);
        }));
    }

    window.next_buffer(
        storage,
        n_frames,
        dropped_cb_wrapper.as_mut(),
        std::ptr::null_mut() as *mut std::ffi::c_void,
    )
}

unsafe fn time_window_put(
    window: &mut MidiTimeWindow,
    storage: &mut MidiStorageCore,
    frame_in_current_buffer: u32,
    size: u16,
    data: *const u8,
    dropped_cb_fn: usize,
    dropped_cb_ctx: usize,
) -> bool {
    let data_slice = std::slice::from_raw_parts(data, size as usize);

    let mut dropped_cb_wrapper: Option<Box<dyn FnMut(u32, u16, *const u8, *mut std::ffi::c_void)>> =
        None;
    if dropped_cb_fn != 0 {
        let cb_fn = dropped_cb_fn;
        let cb_ctx = dropped_cb_ctx;
        dropped_cb_wrapper = Some(Box::new(move |t, s, d, _| {
            let cb: AppendDroppedCb = std::mem::transmute(cb_fn);
            cb(t, s, d, cb_ctx);
        }));
    }

    window.put(
        storage,
        frame_in_current_buffer,
        size,
        data_slice,
        dropped_cb_wrapper.as_mut(),
        std::ptr::null_mut() as *mut std::ffi::c_void,
    )
}

unsafe fn time_window_snapshot(
    window: &MidiTimeWindow,
    storage: &MidiStorageCore,
    target: &mut MidiStorageCore,
    start_offset_from_end: u32,
) {
    window.snapshot(storage, target, start_offset_from_end)
}

// Callback type for truncate_fn predicate
type TruncatePredicate = unsafe extern "C" fn(u32, u16, *const u8, usize) -> bool;

/// Prepend operation - adds a message at the tail (earlier in time)
/// Returns true on success, false if buffer is full or out-of-order
unsafe fn prepend(
    storage: &mut MidiStorageCore,
    time: u32,
    size: u16,
    data: *const u8,
    _dropped_cb_fn: usize,
    _dropped_cb_ctx: usize,
) -> bool {
    let slice = std::slice::from_raw_parts(data, size as usize);
    storage.prepend(time, size, slice)
}

/// Truncate operation with a predicate function
/// The predicate is called for each message and returns true if it should be dropped
unsafe fn truncate_fn(
    storage: &mut MidiStorageCore,
    predicate_fn: usize,
    predicate_ctx: usize,
    side: u8,
    dropped_cb_fn: usize,
    dropped_cb_ctx: usize,
) {
    let truncate_side = if side == 0 {
        TruncateSide::TruncateTail
    } else {
        TruncateSide::TruncateHead
    };

    let capacity = storage.capacity();
    let n_events = storage.n_events();
    if capacity == 0 || n_events == 0 {
        return;
    }

    let pred_fn: Option<TruncatePredicate> = if predicate_fn != 0 {
        Some(std::mem::transmute(predicate_fn))
    } else {
        None
    };

    // Helper to call predicate
    let should_drop = |time: u32, size: u16, data: *const u8| -> bool {
        if let Some(fn_ptr) = pred_fn {
            unsafe { fn_ptr(time, size, data, predicate_ctx) }
        } else {
            false
        }
    };

    // First pass: identify what to drop and collect dropped messages
    let mut to_drop: Vec<MidiStorageElem> = Vec::new();
    let mut kept_tail = storage.raw_tail();
    let mut kept_head = storage.raw_head();
    let mut kept_count = 0u32;

    match truncate_side {
        TruncateSide::TruncateHead => {
            // TruncateHead: drop from the oldest (tail), keep newer (head)
            let mut idx = storage.raw_tail();
            let mut first_keep_idx = idx;
            let mut found_keep = false;

            for _ in 0..n_events {
                if let Some(elem) = storage.get_elem_at_physical_offset_ref(idx) {
                    if !should_drop(elem.time, elem.size, elem.data().as_ptr()) {
                        first_keep_idx = idx;
                        found_keep = true;
                        break;
                    }
                    to_drop.push(elem.clone());
                }
                idx = (idx + 1) % capacity;
            }

            if found_keep {
                kept_tail = first_keep_idx;
                kept_head = storage.raw_head();
                kept_count = n_events - to_drop.len() as u32;
            } else {
                // Drop everything
                to_drop.clear();
                let mut idx = storage.raw_tail();
                for _ in 0..n_events {
                    if let Some(elem) = storage.get_elem_at_physical_offset_ref(idx) {
                        to_drop.push(elem.clone());
                    }
                    idx = (idx + 1) % capacity;
                }
                kept_tail = storage.raw_head();
                kept_head = kept_tail;
                kept_count = 0;
            }
        }
        TruncateSide::TruncateTail => {
            // TruncateTail: drop from the head side, keep older elements
            let mut idx = storage.raw_tail();

            for _ in 0..n_events {
                if let Some(elem) = storage.get_elem_at_physical_offset_ref(idx) {
                    if should_drop(elem.time, elem.size, elem.data().as_ptr()) {
                        to_drop.push(elem.clone());
                    } else {
                        kept_count += 1;
                        kept_head = (idx + 1) % capacity;
                    }
                }
                idx = (idx + 1) % capacity;
            }
        }
    }

    // Update storage state
    storage.set_raw_tail(kept_tail);
    storage.set_raw_head(kept_head);
    storage.set_n_events(kept_count);

    // Call dropped callback for each dropped message
    for elem in &to_drop {
        if dropped_cb_fn != 0 {
            let cb: TruncateDroppedCb = std::mem::transmute(dropped_cb_fn);
            cb(elem.time, elem.size, elem.data().as_ptr(), dropped_cb_ctx);
        }
    }
}
