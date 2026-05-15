//! CXX bridge for MIDI storage to expose to C++.
//!
//! This module provides a minimal FFI bridge between Rust MIDI storage
//! and the C++ backend via CXX.

#![allow(dead_code)]

use crate::midi_storage::{FindResult, MidiCursor, MidiStorageCore, MidiStorageElem, TruncateSide};

// Sentinel value to represent "no value" / invalid offset
const INVALID_OFFSET: u32 = 0xFFFFFFFF;

#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
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

        // MidiStorageElem - element type (shared struct)
        type MidiStorageElem;

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
        fn cursor_find_time_forward(cursor: &mut MidiCursor, storage: &MidiStorageCore, time: u32) -> Box<FindResult>;

        // FindResult - search result type
        type FindResult;

        fn find_result_n_processed(res: &FindResult) -> u32;
        fn find_result_found_valid_elem(res: &FindResult) -> bool;

        // Append operation with optional dropped message
        // Returns: bit 0 = success, bit 1 = dropped
        unsafe fn append(
            storage: &mut MidiStorageCore,
            time: u32,
            size: u16,
            data: *const u8,
            allow_replace: bool,
        ) -> u8;  // lower 8 bits: success (bit 0), dropped (bit 1)
        
        // Get info about the last dropped element from append (if any)
        fn get_last_dropped_elem(storage: &MidiStorageCore) -> u32;  // returns 0 if no dropped, else index into storage
        
        // Truncate preview - returns count of messages that would be dropped
        fn truncate_preview(
            storage: &mut MidiStorageCore,
            time: u32,
            side: u8,  // 0 = TruncateTail, 1 = TruncateHead
        ) -> u32;
        
        // Get preview element at index
        fn get_preview_elem_at(storage: &MidiStorageCore, idx: u32) -> *const MidiStorageElem;
        
        // Get preview element fields
        fn get_preview_elem_time(storage: &MidiStorageCore, idx: u32) -> u32;
        fn get_preview_elem_size(storage: &MidiStorageCore, idx: u32) -> u16;
        unsafe fn get_preview_elem_bytes(storage: &MidiStorageCore, idx: u32, out: *mut u8, max_len: usize);
        
        // Perform actual truncate (call after handling preview)
        fn truncate_doit(
            storage: &mut MidiStorageCore,
            time: u32,
            side: u8,
        );
        
        // Clear preview buffer
        fn clear_preview(storage: &mut MidiStorageCore);
        
        fn clear_storage(storage: &mut MidiStorageCore);
        
        fn copy_to_storage(storage: &MidiStorageCore, target: &mut MidiStorageCore);
        fn copy_from_storage(storage: &mut MidiStorageCore, source: &MidiStorageCore);

        // Data access for syncing C++ state
        fn get_elem_time(storage: &MidiStorageCore, idx: u32) -> u32;
        fn get_elem_size(storage: &MidiStorageCore, idx: u32) -> u16;
        unsafe fn get_elem_bytes(storage: &MidiStorageCore, idx: u32, out: *mut u8, max_len: usize);
    }
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

fn cursor_find_time_forward(cursor: &mut MidiCursor, storage: &MidiStorageCore, time: u32) -> Box<FindResult> {
    Box::new(cursor.find_time_forward(storage, time))
}

// Append operation - returns packed byte with success/dropped flags
unsafe fn append(storage: &mut MidiStorageCore, time: u32, size: u16, data: *const u8, allow_replace: bool) -> u8 {
    let slice = unsafe { std::slice::from_raw_parts(data, size as usize) };
    let result = storage.append(time, size, slice, allow_replace);
    
    // Pack result: bit 0 = success, bit 1 = dropped
    let mut flags = 0u8;
    if result.success {
        flags |= 1;
    }
    if result.dropped {
        flags |= 2;
    }
    flags
}

fn get_last_dropped_elem(storage: &MidiStorageCore) -> u32 {
    storage.get_last_dropped_idx()
}

fn truncate_preview(storage: &mut MidiStorageCore, time: u32, side: u8) -> u32 {
    let side = if side == 0 { TruncateSide::TruncateTail } else { TruncateSide::TruncateHead };
    storage.truncate_preview(time, side)
}

fn get_preview_elem_at(storage: &MidiStorageCore, idx: u32) -> *const MidiStorageElem {
    match storage.get_preview_elem(idx) {
        Some(elem) => elem as *const MidiStorageElem,
        None => std::ptr::null(),
    }
}

fn get_preview_elem_time(storage: &MidiStorageCore, idx: u32) -> u32 {
    storage.get_preview_elem(idx).map(|e| e.time).unwrap_or(0)
}

fn get_preview_elem_size(storage: &MidiStorageCore, idx: u32) -> u16 {
    storage.get_preview_elem(idx).map(|e| e.size).unwrap_or(0)
}

unsafe fn get_preview_elem_bytes(storage: &MidiStorageCore, idx: u32, out: *mut u8, max_len: usize) {
    if let Some(elem) = storage.get_preview_elem(idx) {
        let len = std::cmp::min(elem.size as usize, max_len);
        unsafe {
            std::ptr::copy_nonoverlapping(elem.data().as_ptr(), out, len);
        }
    }
}

fn truncate_doit(storage: &mut MidiStorageCore, time: u32, side: u8) {
    let side = if side == 0 { TruncateSide::TruncateTail } else { TruncateSide::TruncateHead };
    storage.truncate_doit(time, side);
}

fn clear_preview(storage: &mut MidiStorageCore) {
    storage.clear_preview();
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

// Data access for syncing C++ state
fn get_elem_time(storage: &MidiStorageCore, idx: u32) -> u32 {
    storage.get_elem_ref(idx).map(|e| e.time).unwrap_or(0)
}

fn get_elem_size(storage: &MidiStorageCore, idx: u32) -> u16 {
    storage.get_elem_ref(idx).map(|e| e.size).unwrap_or(0)
}

unsafe fn get_elem_bytes(storage: &MidiStorageCore, idx: u32, out: *mut u8, max_len: usize) {
    if let Some(elem) = storage.get_elem_ref(idx) {
        let len = std::cmp::min(elem.size as usize, max_len);
        unsafe {
            std::ptr::copy_nonoverlapping(elem.data().as_ptr(), out, len);
        }
    }
}