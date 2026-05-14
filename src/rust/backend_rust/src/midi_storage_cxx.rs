//! CXX bridge for MIDI storage to expose to C++.
//!
//! This module provides a minimal FFI bridge between Rust MIDI storage
//! and the C++ backend via CXX.

#![allow(dead_code)]

use crate::midi_storage::{FindResult, MidiStorageCore, MidiStorageElem};

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