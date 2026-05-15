//! CXX bridge for MIDI storage to expose to C++.
//!
//! This module provides a minimal FFI bridge between Rust MIDI storage
//! and the C++ backend via CXX.

#![allow(dead_code)]

use crate::midi_storage::{FindResult, MidiCursor, MidiStorageCore, MidiStorageElem};

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